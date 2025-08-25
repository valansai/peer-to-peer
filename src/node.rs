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
use tokio::sync::Mutex;
use std::sync::Arc;

use nymlib::serialize::{
    Serialize, 
    DataStream
};


use crate::net_msg_header::
{
    MessageHeader,
    MESSAGE_HEADER_CONFIG
};

use log::info;


use crate::net_addr::Address;
use crate::util::get_time;



pub mod COMMANDS {
    pub const VERSION: &str = "VERSION";   // Version handshake message
    pub const VERACK: &str = "VERACK";     // Version acknowledgment
    pub const PING: &str = "PING";         // Keep-alive ping
    pub const PONG: &str = "PONG";         // Response to ping
    pub const ADDR: &str = "ADDR";         // Request addresses from a peer
    pub const GETADDR: &str = "GETADDR";   // Send known addresses to a requesting peer
}



#[derive(Debug, Default)]
pub struct NodeStats {
    pub last_time_send: u64,     // Timestamp of the last successful send
    pub last_time_recv: u64,     // Timestamp of the last successful receive
    pub total_bytes_send: u64,   // Total number of bytes sent
    pub total_bytes_recv: u64,   // Total number of bytes received
    pub last_ping_time: u64,     // Timestamp of the last ping sent
}

#[derive(Debug)]
pub struct Node {
    pub addr: Address,                       // Node address and advertised services
    pub version: u32,                        // Node version
    pub data_recv: Arc<Mutex<DataStream>>,   // Data received from this node
    pub data_send: Arc<Mutex<DataStream>>,   // Data queued to be sent to this node
    pub push_position: i32,                  // Current push position in the send buffer
    pub inbound: bool,                       // true if connection is inbound, false if outbound
    pub already_send_addrs: Vec<[u8; 32]>,   // List of addreses that already have been sent to this node
    pub stats: NodeStats,                    // Statistics for network activity
}


impl Node {
    pub async fn new(addr: Address, push_version: Option<bool>) -> Self {
        let inbound = push_version != Some(true); 

        let mut node = Self {
            addr,
            version: 0,
            data_recv: Arc::new(Mutex::new(DataStream::default())),
            data_send: Arc::new(Mutex::new(DataStream::default())),
            push_position: -1,
            inbound, 
            already_send_addrs: Vec::new(),
            stats: NodeStats::default(),
        };


        node.stats.last_time_recv = get_time(); 

        if !inbound {
            node.push_message(COMMANDS::VERSION, (1u32,)).await;
        }
        node
    } 

    pub async fn push_message<T>(&mut self, command: &str, args: T) -> io::Result<()>

    where
        T: Serialize,

    {
        info!("[*] Pushing {} to {:?}", command, self.addr.address);
        self.begin_message(command, false).await?;
        {
            let mut data_send = self.data_send.lock().await;
            data_send.stream_in(&args)?;
        }
        self.end_message().await
    }

    pub async fn begin_message(&mut self, command: &str, ignore: bool) -> io::Result<()> {
        if ignore { return Ok(()); }

        let mut data_send = self.data_send.lock().await;
        self.push_position = data_send.size() as i32;

        let header = MessageHeader::new(Some(command), Some(0));
        data_send.stream_in(&header)
    }

    pub async fn end_message(&mut self) -> io::Result<()> {
        if self.push_position == -1 { return Ok(()); }

        let mut data_send = self.data_send.lock().await;
        let n_size = (data_send.size() as i32 - MESSAGE_HEADER_CONFIG::MESSAGE_HEADER_SIZE as i32) as u32;

        let offset = (self.push_position + 16) as usize; 
        data_send.raw_write(&n_size.to_le_bytes(), offset);

        self.push_position = -1;
        Ok(())
    }
    
    pub fn is_anonymous(&self) -> bool {
        self.addr.is_anonymous()
    }

    pub fn is_individual(&self) -> bool {
        self.addr.is_individual()
    }
}