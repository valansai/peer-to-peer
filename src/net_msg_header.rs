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


use std::io;
use nymlib::serialize::Serialize;
use nymlib::serialize_derive::impl_serialize_for_struct;




// Config values for message header
pub mod MESSAGE_HEADER_CONFIG {
    pub const MESSAGE_START: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xF1];   // network magic bytes
    pub const MESSAGE_SIZE: u32 = 0;                               // default size
    pub const MESSAGE_HEADER_COMMAND_SIZE: usize = 12;             // command length
    pub const MESSAGE_HEADER_SIZE: usize = MESSAGE_START.len() +
        MESSAGE_HEADER_COMMAND_SIZE  + 4;                          // total header size
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageHeader {
    pub message_start: [u8; 4],  // start bytes
    pub command: [u8; 12],       // command name
    pub message_size: u32,       // payload size
}

impl MessageHeader {
    // make new header
    pub fn new(command: Option<&str>, message_size: Option<u32>) -> Self {
        let mut command_array = [0u8; 12];
        if let Some(cmd) = command {
            let bytes = cmd.as_bytes();
            let len = bytes.len().min(12);
            command_array[..len].copy_from_slice(&bytes[..len]);
        }

        Self {
            message_start: MESSAGE_HEADER_CONFIG::MESSAGE_START,
            command: command_array,
            message_size: message_size.unwrap_or(MESSAGE_HEADER_CONFIG::MESSAGE_SIZE),
        }
    }

    // get command as string
    pub fn command(&self) -> Result<&str, std::str::Utf8Error> {
        let end = self.command.iter().position(|&b| b == 0).unwrap_or(self.command.len());
        std::str::from_utf8(&self.command[..end])
    }

    // check if header is valid
    pub fn is_valid(&self) -> bool {
        if self.message_start != MESSAGE_HEADER_CONFIG::MESSAGE_START {
            return false;
        }

        let mut null_found = false;
        for (i, &byte) in self.command.iter().enumerate() {
            if byte == 0 {
                null_found = true;
                if self.command[i + 1..].iter().any(|&b| b != 0) {
                    return false;
                }
                break;
            }
            if !(0x20..=0x7E).contains(&byte) {
                return false;
            }
        }

        null_found || self.command.len() <= MESSAGE_HEADER_CONFIG::MESSAGE_HEADER_COMMAND_SIZE
    }

    // size of header when serialized
    pub fn get_serialize_size(&self, _n_type: u32, _n_version: u32) -> usize {
        MESSAGE_HEADER_CONFIG::MESSAGE_START.len() +
            MESSAGE_HEADER_CONFIG::MESSAGE_HEADER_COMMAND_SIZE +
            std::mem::size_of::<u32>() +
            std::mem::size_of::<u64>()
    }
}

impl Default for MessageHeader {
    fn default() -> Self {
        Self {
            message_start: MESSAGE_HEADER_CONFIG::MESSAGE_START,
            command: [0u8; 12],
            message_size: MESSAGE_HEADER_CONFIG::MESSAGE_SIZE,
        }
    }
}

impl_serialize_for_struct! {
    target MessageHeader {
        readwrite(self.message_start);
        readwrite(self.command);
        readwrite(self.message_size);
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use nymlib::serialize::{
        Serialize, 
        DataStream
    };


    #[test]
    fn test_new_valid() {
        // Test creating a valid header
        let hdr = MessageHeader::new(Some("VERSION"), Some(100));
        assert_eq!(hdr.message_start, MESSAGE_HEADER_CONFIG::MESSAGE_START);
        assert_eq!(hdr.command, [86, 69, 82, 83, 73, 79, 78, 0, 0, 0, 0, 0]);
        assert_eq!(hdr.message_size, 100);
        assert!(hdr.is_valid());
    }

    #[test]
    fn test_new_no_command() {
        // Test creating a header with no command
        let hdr = MessageHeader::new(None, None);
        assert_eq!(hdr.command, [0; 12]);
        assert_eq!(hdr.message_size, 0);
        assert!(hdr.is_valid());
    }

    #[test]
    fn test_is_valid_invalid_start() {
        // Test invalid message_start
        let mut hdr = MessageHeader::new(Some("VERSION"), Some(0));
        hdr.message_start = [0, 0, 0, 0];
        assert!(!hdr.is_valid());
    }

    #[test]
    fn test_default() {
        // Test Default implementation
        let hdr = MessageHeader::default();
        assert_eq!(hdr.message_start, MESSAGE_HEADER_CONFIG::MESSAGE_START);
        assert_eq!(hdr.command, [0; 12]);
        assert_eq!(hdr.message_size, 0);
        assert!(hdr.is_valid());
    }

    #[test]
    fn test_serialization_with_payload() {
        // Test serialization and deserialization of header with i32 payload
        let version: i32 = 1;
        let hdr = MessageHeader::new(Some("VERSION"), Some(4)); // i32 is 4 bytes

        // Serialize header and payload
        let mut stream = DataStream::default();
        stream.stream_in(&hdr);
        stream.stream_in(&version);
        let serialized = stream.data;

        // Deserialize header and payload
        let mut stream_out = DataStream::default();
        stream_out.write(&serialized);
        let deserialized_hdr = stream_out
            .stream_out::<MessageHeader>()
            .expect("Header deserialization failed");

        // VERSION" Command: Expected to have an i32 payload (4 bytes) representing the remote
        // node protocol version. 
        if deserialized_hdr.command() == Ok("VERSION") {
            let deserialized_version = stream_out
                .stream_out::<i32>()
                .expect("Version deserialization failed");

            // Do something.... Not much this is a test case.
        }
    }

    #[test]
    fn test_serialization_with_cursor() {
        // Test serialization and deserialization using Serialize trait
        let hdr = MessageHeader::new(Some("VERSION"), Some(100));
        let mut buffer = Vec::new();
        hdr.serialize(&mut buffer, 0, 0).expect("Serialization failed");
        let mut cursor = Cursor::new(buffer);
        let mut deserialized = MessageHeader::default();
        deserialized.unserialize(&mut cursor, 0, 0).expect("Deserialization failed");
        assert_eq!(hdr, deserialized, "Deserialized header does not match original");
    }


    #[test]
    fn test_serialization_with_datastream() {
        // Test serialization and deserialization using DataStream
        let hdr = MessageHeader::new(Some("VERSION"), Some(100));
        let mut stream = DataStream::default();
        stream.stream_in(&hdr);
        let serialized = stream.data;

        let mut stream_out = DataStream::default();
        stream_out.write(&serialized);
        let deserialized = stream_out.stream_out::<MessageHeader>().expect("DataStream deserialization failed");

        assert_eq!(hdr, deserialized, "Deserialized header does not match original");
    }

    #[test]
    fn test_serialization_consistency() {
        // Test that Serialize trait and DataStream produce the same serialized output
        let hdr = MessageHeader::new(Some("VERSION"), Some(100));
        
        // Serialize with Serialize trait
        let mut buffer = Vec::new();
        hdr.serialize(&mut buffer, 0, 0).expect("Serialization failed");

        // Serialize with DataStream
        let mut stream = DataStream::default();
        stream.stream_in(&hdr);

        assert_eq!(buffer, stream.data, "Serialized output differs between Serialize and DataStream");
    }
}