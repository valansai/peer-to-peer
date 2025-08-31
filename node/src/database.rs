// MIT License
// Copyright (c) Valan Sai 2025
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.


use lmdb::
{
    Environment, 
    Database, 
    WriteFlags, 
    Cursor, 
    Transaction
};

use nymlib::serialize::
{
    Serialize, 
    Deserialize, 
    DataStream, 
    SER_DISK, 
    VERSION
};

use log::warn;
use std::path::Path;
use std::io;




#[derive(Debug)]
pub enum DBError {
    Lmdb {
        error: lmdb::Error,
        database: Option<String>,
        key: Option<Vec<u8>>,
    },
    Io {
        error: std::io::Error,
        database: Option<String>,
        key: Option<Vec<u8>>,
    },
}

impl DBError {
    fn lmdb(error: lmdb::Error, database: Option<&str>, key: Option<&[u8]>) -> Self {
        DBError::Lmdb {
            error,
            database: database.map(String::from),
            key: key.map(|k| k.to_vec()),
        }
    }

    fn io(error: std::io::Error, database: Option<&str>, key: Option<&[u8]>) -> Self {
        DBError::Io {
            error,
            database: database.map(String::from),
            key: key.map(|k| k.to_vec()),
        }
    }
}

impl std::fmt::Display for DBError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DBError::Lmdb { error, database, key } => {
                write!(f, "LMDB error: {}", error)?;
                if let Some(db) = database {
                    write!(f, " in database '{}'", db)?;
                }
                if let Some(k) = key {
                    write!(f, " for key {:?}", k)?;
                }
                Ok(())
            }
            DBError::Io { error, database, key } => {
                write!(f, "I/O error: {}", error)?;
                if let Some(db) = database {
                    write!(f, " in database '{}'", db)?;
                }
                if let Some(k) = key {
                    write!(f, " for key {:?}", k)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for DBError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DBError::Lmdb { error, .. } => Some(error),
            DBError::Io { error, .. } => Some(error),
        }
    }
}

impl From<lmdb::Error> for DBError {
    fn from(err: lmdb::Error) -> Self {
        DBError::Lmdb {
            error: err,
            database: None,
            key: None,
        }
    }
}

impl From<std::io::Error> for DBError {
    fn from(err: std::io::Error) -> Self {
        DBError::Io {
            error: err,
            database: None,
            key: None,
        }
    }
}



#[derive(Debug, Clone)]
pub struct TypedKey<K> {
    type_name: String,
    key: K,
}

pub trait KeyValueInput<K: Serialize, V: Serialize> {
    fn into_pairs(self) -> Vec<(TypedKey<K>, V)>;
}

impl<K: Serialize, V: Serialize> KeyValueInput<K, V> for (K, V) {
    fn into_pairs(self) -> Vec<(TypedKey<K>, V)> {
        let (key, value) = self;
        let type_name = get_type_prefix::<V>();
        vec![(TypedKey { type_name, key }, value)]
    }
}

impl<K: Serialize + Clone, V: Serialize + Clone> KeyValueInput<K, V> for &[(K, V)] {
    fn into_pairs(self) -> Vec<(TypedKey<K>, V)> {
        let type_name = get_type_prefix::<V>();
        self.iter()
            .map(|(k, v)| (TypedKey { type_name: type_name.clone(), key: k.clone() }, v.clone()))
            .collect()
    }
}


fn get_type_prefix<V: Serialize>() -> String {
    let full_type_name = std::any::type_name::<V>();
    let type_name = full_type_name
        .split("::")
        .last()
        .unwrap_or("unknown")
        .to_lowercase();
    format!("{}:", type_name)
}



#[derive(Debug)]
pub struct DBWrapper {
    env: Environment,
    db: Database,
    manual_mode: bool,
    pending_writes: Vec<(Vec<u8>, Vec<u8>)>,
}

impl DBWrapper {
    pub fn new(path: &str, db_name: &str, manual_mode: bool, map_size: usize) -> Result<Self, DBError> {
        let env = Environment::new()
            .set_max_dbs(1) 
            .set_map_size(map_size)
            .open(Path::new(path))
            .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;

        let db = env
            .create_db(Some(db_name), lmdb::DatabaseFlags::empty())
            .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;

        Ok(Self {
            env,
            db,
            manual_mode,
            pending_writes: Vec::new(),
        })
    }

    pub fn put<K: Serialize + Clone, V: Serialize + Clone, I: KeyValueInput<K, V>>(
        &mut self,
        input: I,
    ) -> Result<(), DBError> {
        let pairs = input.into_pairs();
        let db_name = "db";
        let mut serialized_entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        for (typed_key, value) in pairs {
            let mut key_stream = DataStream::default();
            (&mut key_stream << typed_key.type_name << typed_key.key);
            let key_bytes = key_stream.data;
            let value_bytes = value
                .to_datastream(SER_DISK, VERSION)
                .map_err(|e| DBError::io(e, Some(db_name), Some(&key_bytes)))?
                .data;
            serialized_entries.push((key_bytes, value_bytes));
        }

        if self.manual_mode {
            self.pending_writes.extend(serialized_entries);
            Ok(())
        } else {
            let mut txn = self
                .env
                .begin_rw_txn()
                .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;
            for (key_bytes, value_bytes) in serialized_entries {
                txn.put(self.db, &key_bytes, &value_bytes, WriteFlags::empty())
                    .map_err(|e| DBError::lmdb(e, Some(db_name), Some(&key_bytes)))?;
            }
            txn.commit()
                .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;
            Ok(())
        }
    }

    pub fn read<K: Serialize + Clone + ?Sized, V: Serialize + Deserialize>(
        &self,
        key: &K,
    ) -> Result<Option<V>, DBError> {
        let type_name = get_type_prefix::<V>();
        let db_name = "db";
        let mut key_stream = DataStream::default();
        (&mut key_stream << type_name << key.clone());
        let key_bytes = key_stream.data;

        let txn = self
            .env
            .begin_ro_txn()
            .map_err(|e| DBError::lmdb(e, Some(db_name), Some(&key_bytes)))?;

        match txn.get(self.db, &key_bytes) {
            Ok(value) => {
                let mut reader = std::io::Cursor::new(value);
                V::deserialize(&mut reader, SER_DISK, VERSION)
                    .map(Some)
                    .map_err(|e| DBError::io(e, Some(db_name), Some(&key_bytes)))
            }
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => Err(DBError::lmdb(e, Some(db_name), Some(&key_bytes))),
        }
    }

    pub fn iter<V: Serialize + Deserialize>(&self) -> Result<Vec<V>, DBError> {
        let type_prefix = get_type_prefix::<V>();
        let db_name = "db";
        let mut prefix_stream = DataStream::default();
        (&mut prefix_stream << type_prefix);
        let prefix_bytes = prefix_stream.data;

        let txn = self
            .env
            .begin_ro_txn()
            .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;
        let mut cursor = txn
            .open_ro_cursor(self.db)
            .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;

        let mut results = Vec::new();
        for (key, value) in cursor.iter() {
            if key.starts_with(&prefix_bytes) {
                let mut reader = std::io::Cursor::new(value);
                let deserialized = V::deserialize(&mut reader, SER_DISK, VERSION)
                    .map_err(|e| DBError::io(e, Some(db_name), Some(key)))?;
                results.push(deserialized);
            } else {
                warn!("Skipping key with unexpected type prefix in {}: {:?}", db_name, key);
            }
        }
        Ok(results)
    }

    pub fn commit(&mut self) -> Result<(), DBError> {
        if !self.manual_mode {
            return Ok(());
        }
        let writes = std::mem::take(&mut self.pending_writes);
        if writes.is_empty() {
            return Ok(());
        }

        let db_name = "db";
        let mut txn = self
            .env
            .begin_rw_txn()
            .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;
        for (key_bytes, value_bytes) in writes {
            txn.put(self.db, &key_bytes, &value_bytes, WriteFlags::empty())
                .map_err(|e| DBError::lmdb(e, Some(db_name), Some(&key_bytes)))?;
        }
        txn.commit()
            .map_err(|e| DBError::lmdb(e, Some(db_name), None))?;
        Ok(())
    }
}

macro_rules! define_databases {
    ($($name:ident => $str:expr),* $(,)?) => {
        $(
            #[derive(Debug)]
            pub struct $name {
                wrapper: DBWrapper,
            }

            impl $name {
                pub fn new(path: &str, manual_mode: bool, map_size: usize) -> Result<Self, DBError> {
                    Ok(Self {
                        wrapper: DBWrapper::new(path, $str, manual_mode, map_size)?,
                    })
                }

                pub fn put<K: Serialize + Clone, V: Serialize + Clone, I: KeyValueInput<K, V>>(
                    &mut self,
                    input: I,
                ) -> Result<(), DBError> {
                    self.wrapper.put(input)
                }

                pub fn read<K: Serialize + Clone + ?Sized, V: Serialize + Deserialize>(
                    &self,
                    key: &K,
                ) -> Result<Option<V>, DBError> {
                    self.wrapper.read(key)
                }

                pub fn iter<V: Serialize + Deserialize>(&self) -> Result<Vec<V>, DBError> {
                    self.wrapper.iter()
                }

                pub fn commit(&mut self) -> Result<(), DBError> {
                    self.wrapper.commit()
                }
            }
        )*
    };
}

define_databases! {
    AddressDB => "address_db",
}