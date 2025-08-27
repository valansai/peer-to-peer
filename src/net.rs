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


use nymlib::nymsocket::{Socket, SocketMode};
use nymlib::serialize::{DataStream, GetHash};
use log::{debug, info, warn, error};
use tokio::sync::broadcast;
use tokio::fs;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::signal;
use std::sync::LazyLock;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;


use crate::node::{Node, COMMANDS};
use crate::net_addr::Address;

use crate::util::{
    init_logging,
    get_time,
};

use crate::net_msg_header::MessageHeader;
use crate::net_msg_header::MESSAGE_HEADER_CONFIG;

// Global state for the node
pub static SOCKET: LazyLock<Mutex<Option<Arc<Mutex<Socket>>>>> = 
    LazyLock::new(|| Mutex::new(None));

pub static LOCAL_ADDR: LazyLock<Mutex<Option<Arc<Mutex<Address>>>>> = 
    LazyLock::new(|| Mutex::new(None));

pub static VNODES: LazyLock<Arc<Mutex<Vec<Arc<Mutex<Node>>>>>> = 
    LazyLock::new(|| Arc::new(Mutex::new(Vec::new()))); 

pub static VADDRESSES: LazyLock<Arc<Mutex<Vec<Address>>>> = 
    LazyLock::new(|| Arc::new(Mutex::new(Vec::new()))); 

pub static NODE_RUNNING: LazyLock<Arc<Mutex<bool>>> = 
    LazyLock::new(|| Arc::new(Mutex::new(false))); 

pub static STOP_SIGNAL: LazyLock<Arc<Mutex<Option<broadcast::Sender<bool>>>>> = 
    LazyLock::new(|| Arc::new(Mutex::new(None))); 



// Default configuration for the node
pub mod DEFAULT_NODE_CONFIG {  
    use nymlib::nymsocket::SocketMode; 
    
    pub const MODE: SocketMode = SocketMode::Individual;  
    pub const DATA_DIR: &str = "nyxnet";     
    pub const DEBUG_LOG_FILE: &str = "debug.log"; 
}

// Look for a node in VNODES by its address
pub async fn find_node<A: Into<Address>>(addr: A) -> Option<Arc<Mutex<Node>>> {
    let addr: Address = addr.into(); 
    let vnodes = VNODES.lock().await;

    for node in vnodes.iter() {
        let node_guard = node.lock().await;
        if node_guard.addr == addr {
            return Some(node.clone());
        }
    }
    None
}

// Add a peer address if not already in VADDRESSES and to addresses.txt
pub async fn add_peer_address(addr: Address) {
    let mut vaddresses = VADDRESSES.lock().await;
    if !vaddresses.iter().any(|a| a.address == addr.address) {
        vaddresses.push(addr.clone()); // runtime 

        // Persist peer addresses to addresses.txt so we can be reused
        // across runs, rather than being lost when the node shuts down.
        let addr_str = addr.address.to_string();
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open("addresses.txt")
            .await 
            .expect("[*] add_peer_address() - Failed to open addresses.txt");
        file.write_all(format!("{}\n", addr_str).as_bytes())
            .await
            .expect("[*] add_peer_address() - Failed to write to addresses.txt");
    }
}


// Load addresses from a file
pub async fn load_addresses(file_path: &str) -> std::io::Result<()> {
    let contents = fs::read_to_string(file_path).await.map_err(|e| {
        std::io::Error::new(e.kind(), format!("Failed to read {}: {}", file_path, e))
    })?;

    let mut collected = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        collected.push(Address::new(line.into(), 0));
    }

    {
        let mut vaddresses = VADDRESSES.lock().await;
        *vaddresses = collected;
    }

    Ok(())
}

// Try to establish outbound connections to peers
pub async fn open_connections() -> std::io::Result<()> {
    let mut stop_signal_rx = STOP_SIGNAL.lock().await.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotConnected, "Stop signal not initialized")
    })?.subscribe();

    info!("[*] open_connections started");

    // Get our local socket address
    let local_addr = {
        let local_addr_opt = LOCAL_ADDR.lock().await;
        match &*local_addr_opt {
            Some(p_addr) => Some(p_addr.lock().await.address.clone()),
            None => None,
        }
    };

    // Run every 30 seconds
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    interval.tick().await; 

    loop {
        tokio::select! {
            // Stop if signal received
            result = stop_signal_rx.recv() => {
                match result {
                    Ok(true) => {
                        info!("[*] Stopping open_connections task");
                        break Ok(());
                    }
                    Ok(false) => continue,
                    Err(_) => break Ok(()),
                }
            }
            // Try connecting to new peers
            _ = interval.tick() => {
                let vnodes_len = {
                    let vnodes = VNODES.lock().await;
                    vnodes.len()
                };

                // Limit max peers
                if vnodes_len >= 10 {
                    continue;
                }

                let addresses = VADDRESSES.lock().await.clone();
                for addr in addresses {
                    // Skip if already connected
                    if find_node(addr.clone()).await.is_some() {
                        continue;
                    }
                    
                    // Skip if our own address
                    if let Some(local) = &local_addr {
                        if addr.address == *local {
                            continue;
                        }
                    }

                    // Skip if recently failed
                    let current_time = get_time();
                    if addr.last_failed_time != 0 && current_time - addr.last_failed_time < 300 {
                        continue;
                    }

                    // Create new node
                    let node = Node::new(addr.clone(), Some(true)).await;
                    let node = Arc::new(Mutex::new(node));

                    {
                        // Add node to VNODES
                        let mut vnodes = VNODES.lock().await;
                        vnodes.push(node);
                    }
                }
            }
        }
    }
}

// Periodically send messages queued for nodes
pub async fn send_messages() -> std::io::Result<()> {
    let mut stop_signal_rx = STOP_SIGNAL.lock().await.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotConnected, "Stop signal not initialized")
    })?.subscribe();

    info!("[*] send_messages started");

    loop {
        tokio::select! {
            // Stop if signal received
            result = stop_signal_rx.recv() => {
                match result {
                    Ok(true) => {
                        info!("[*] send_messages() - stopping send_messages task");
                        break Ok(());
                    }
                    Ok(false) => continue,
                    Err(_) => break Ok(()),
                }
            }
            // Every 10 seconds send queued messages
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                let socket_opt = SOCKET.lock().await;
                let Some(p_socket) = &*socket_opt else {
                    continue;
                };

                let nodes = VNODES.lock().await;
                for node in nodes.iter() {
                    let mut node = node.lock().await;
                    let mut data_send = node.data_send.lock().await;

                    if !data_send.empty() {
                        let message = data_send.unread_str().to_vec();
                        let mut socket = p_socket.lock().await;
                        if socket.send(message, node.addr.clone()).await {
                            data_send.data.clear();
                        } else {
                            debug!("[send_messages] Failed to send message to {}", node.addr);
                        }
                    }
                }
            }
        }
    }
}



// Handle incoming messages from the socket
pub async fn handle_messages() -> std::io::Result<()> {
    let mut stop_signal_rx = STOP_SIGNAL.lock().await.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotConnected, "Stop signal not initialized")
    })?.subscribe();

    info!("[*] handle_messages started");

    // Run every 5 seconds
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    interval.tick().await; 

    loop {
        tokio::select! {
            result = stop_signal_rx.recv() => {
                match result {
                    Ok(true) => {
                        info!("[*] Stopping handle_messages task");
                        break Ok(());
                    }
                    Ok(false) => continue,
                    Err(_) => break Ok(()),
                }
            }

            // Check for new messages from socket
            _ = interval.tick() => {
                let socket_opt = SOCKET.lock().await;
                let Some(p_socket) = &*socket_opt else {
                    debug!("[*] Socket not initialized; skipping message handling");
                    continue;
                };

                let messages = {
                    let mut socket = p_socket.lock().await;            
                    let mut recv = socket.recv.lock().await;           
                    let msgs = recv.clone();                           
                    recv.clear(); // Clear after taking messages
                    msgs   
                };

                // Process each message
                for message in messages {
                    let from_addr = message.from; 

                    // If from known node, update its recv buffer
                    if let Some(node) = find_node(from_addr.clone()).await {
                        info!("[*] Processing message from already connected node, addr: {:?}", from_addr.to_string());
                        let mut node_guard = node.lock().await;
                        node_guard.data_recv.lock().await.write(&message.data); // write the received message 
                        node_guard.stats.last_time_recv = get_time();  // update its last time recv 
                    } else 
                    {
                        // New node, create and add it
                        info!("[*] Processing message from a new node, addr: {:?}", from_addr.to_string());

                        let node = Node::new(from_addr.clone().into(), None).await;
                        let node = Arc::new(Mutex::new(node));

                        {
                            let mut node_guard = node.lock().await;
                            node_guard.data_recv.lock().await.stream_in(&message.data); // write the received message 
                            node_guard.stats.last_time_recv = get_time();  // update its last time recv
                        }


                        // Add node to VNODES
                        let mut vnodes = VNODES.lock().await;
                        vnodes.push(node);
                    }
                }
            }
        }
    }
}

// Process buffered messages into commands
pub async fn process_messages() -> std::io::Result<()> {
    let mut stop_signal_rx = STOP_SIGNAL.lock().await.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotConnected, "Stop signal not initialized")
    })?.subscribe();

    info!("[*] process_messages started");

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            result = stop_signal_rx.recv() => {
                match result {
                    Ok(true) => {
                        info!("[*] Stopping process_messages task");
                        break Ok(());
                    }
                    Ok(false) => continue,
                    Err(_) => break Ok(()),
                }
            }
            _ = interval.tick() => {
                let nodes = VNODES.lock().await;

                for node in nodes.iter() {
                    let mut node_guard = node.lock().await;
                    let received_arc = node_guard.data_recv.clone();
                    let mut received = received_arc.lock().await;

                    if received.size() == 0 {
                        continue;
                    }

                    // Look for message start sequence
                    let data = received.unread_str();
                    let start = data
                        .windows(MESSAGE_HEADER_CONFIG::MESSAGE_START.len())
                        .position(|window| window == MESSAGE_HEADER_CONFIG::MESSAGE_START)
                        .map(|idx| received.cursor + idx)
                        .unwrap_or_else(|| received.end_index());

                    // If not enough data for header, cleanup and skip
                    if received.end_index() - start < MESSAGE_HEADER_CONFIG::MESSAGE_HEADER_SIZE {
                        if received.size() > MESSAGE_HEADER_CONFIG::MESSAGE_HEADER_SIZE {
                            let begin_index = received.begin_index();
                            let end_index = received.end_index() - MESSAGE_HEADER_CONFIG::MESSAGE_HEADER_SIZE;
                            received.erase(begin_index, Some(end_index));
                            debug!("[*] Insufficient data for header from {:?}, erasing {} bytes", node_guard.addr, end_index - begin_index);
                        }
                        continue;
                    }

                    // Drop data before start marker
                    let begin_index = received.begin_index();
                    if start > begin_index {
                        received.erase(begin_index, Some(start));
                    }

                    let recv_copy = received.copy();

                    // Try to parse header
                    match received.stream_out::<MessageHeader>() {
                        Ok(hdr) => {
                            if !hdr.is_valid() {
                                info!("[*] Invalid header from {:?}", node_guard.addr);
                                received.data = recv_copy.data;
                                continue;
                            }

                            let command = match hdr.command() {
                                Ok(cmd) => cmd,
                                Err(e) => {
                                    debug!("[*] Failed to parse command from {:?}: {}", node_guard.addr, e);
                                    received.data = recv_copy.data;
                                    continue;
                                }
                            };

                            // Check message size
                            let message_size: usize = hdr.message_size as usize;
                            if message_size > received.size() {
                                info!("[*] Insufficient data for message from {:?} (need {}, have {})", node_guard.addr, message_size, received.size());
                                received.data = recv_copy.data;
                                continue;
                            }

                            // Copy data into DataStream
                            let remaining_size = received.size() - received.begin_index();
                            let raw_buf = received.raw_read_buf(received.begin_index(), remaining_size);
                            let mut msg = DataStream::default();
                            if let Err(e) = msg.write(&raw_buf[..message_size]) {
                                info!("[*] Failed to write to DataStream from {:?}: {}", node_guard.addr, e);
                                received.data = recv_copy.data;
                                continue;
                            }
                            received.ignore(message_size);

                            // Process message command
                            if !process_message(&mut node_guard, &command, msg).await {
                                info!("[*] Process message with command {} from {:?} returned false", command, node_guard.addr);
                            }

                            received.compact();
                        }
                        Err(e) => {
                            info!("[*] Failed to read header from {:?}: {}", node_guard.addr, e);
                            received.data = recv_copy.data;
                            continue;
                        }
                    }
                }
            }
        }
    }
}

pub async fn process_message(node: &mut tokio::sync::MutexGuard<'_, Node>, command: &str, mut msg: DataStream) -> bool {
    // Process a message received from a peer node based on its command
    match command {
        COMMANDS::VERSION => {
            // Handle VERSION message
            info!("[*] Received a VERSION message from {}", node.addr);
            if node.version != 0 {
                // Ignore duplicate VERSION messages
                info!("[*] Duplicate VERSION message from {}", node.addr);
                return false; 
            }

            match msg.stream_out::<u32>() {
                Ok(version) => {
                    if version == 1 {
                        node.version = version; // Set the node version
                        node.push_message(COMMANDS::VERACK, ()).await; // Queue a VERACK message

                        // If node is individual, add its address to known peers
                        if node.is_individual() {
                            add_peer_address(Address {
                                address: node.addr.address.clone(), 
                                services: node.addr.services, 
                                last_failed_time: 0, 
                            }).await;
                        }
                        true 
                    } else {
                        false 
                    }
                }
                Err(_) => false // Failed to parse version
            }
        }

        COMMANDS::VERACK => {
            // Handle VERACK message
            info!("[*] Received VERACK message from {}", node.addr);
            // Queue a ADDR message, here we ask remote peer for addresses
            node.version = 1; // consider same version 
            true 
        }

        COMMANDS::ADDR => {
            // Handle ADDR message, send addresses to peer, filter out addresses already sent
            info!("[*] Received ADDR message from {}", node.addr);
            let addresses: Vec<Address> = {
                let vaddresses = VADDRESSES.lock().await;
                vaddresses.clone()
            };

            let new_addresses: Vec<Address> = addresses
                .into_iter()
                .filter(|addr| !node.already_send_addrs.contains(&addr.get_hash()))
                .collect();

            if !new_addresses.is_empty() {
                // Update already sent addresses
                let mut already_sent = node.already_send_addrs.clone();
                for addr in &new_addresses {
                    let hash = addr.get_hash();
                    if !already_sent.contains(&hash) {
                        already_sent.push(hash);
                    }
                }
                node.already_send_addrs = already_sent;
            }

            // Queue a GETADDR message, here send to peer addresses          
            node.push_message(COMMANDS::GETADDR, new_addresses).await; 
            true 
        }

        COMMANDS::GETADDR => {
            // Handle GETADDR message
            info!("[*] Received GETADDR message from {}", node.addr);

            let local_addr = {
                let local_addr_opt = LOCAL_ADDR.lock().await;
                match &*local_addr_opt {
                    Some(p_addr) => Some(p_addr.lock().await.address.clone()),
                    None => None,
                }
            };

            match msg.stream_out::<Vec<Address>>() {
                Ok(addresses) => {
                    if !addresses.is_empty() {
                        let mut vaddresses = VADDRESSES.lock().await;

                        for addr in addresses {
                            // Skip local address
                            if let Some(local) = &local_addr {
                                if addr.address == *local {
                                    info!("[*] GETADDR - Skipping local address");
                                    continue; 
                                }
                            }
                            // Add new addresses to VADDRESSES, if not already have
                            if !vaddresses.iter().any(|existing| existing.address == addr.address) {
                                info!("[*] GETADDR - Added {:?}", addr.address);
                                vaddresses.push(addr.clone()); 
                            }
                        }
                        true 
                    } else {
                        true 
                    }
                }
                Err(_) => false
            }
        }

        COMMANDS::PING => {
            info!("[*] Received PING message from {}", node.addr);
            // Queue a PONG message
            node.push_message(COMMANDS::PONG, ()).await; 
            true 
        }

        COMMANDS::PONG => {
            // Handle PONG message, already hanlded by handle_messages, we update node last time recv
            info!("[*] Received PONG message from {}", node.addr);
            true 
        }

        _ => {
            // Unknown command
            info!("[*] Received unknown message {} from {}", command, node.addr);
            false 
        }
    }
}

async fn node_manager() -> std::io::Result<()> {
    // Task to monitor nodes, curently request peer addresses from already conected nodes
    let mut stop_signal_rx = STOP_SIGNAL.lock().await.as_ref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotConnected, "Stop signal not initialized")
    })?.subscribe();

    info!("[*] node_manager started");

    let mut peer_request_interval = tokio::time::interval(tokio::time::Duration::from_secs(240));
    peer_request_interval.tick().await;

    loop {
        tokio::select! {
            // Handle stop signal
            result = stop_signal_rx.recv() => {
                match result {
                    Ok(true) => {
                        info!("[*] Stopping node_manager task");
                        break Ok(());
                    }
                    Ok(false) => continue,
                    Err(e) => {
                        debug!("[*] Stop signal error in node_manager: {}", e);
                        break Ok(());
                    }
                }
            }
            
            _ = peer_request_interval.tick() => {
                // Every 240 seconds (4 minutes) request peer addresses from connected nodes
                info!("[*] Requesting peer addresses");

                // Get the current number of connected nodes
                let vnodes_len = {
                    let vnodes = VNODES.lock().await; 
                    vnodes.len() // Count the number of nodes
                };

                // Only proceed with address requests if we have fewer than 10 connected peers. 
                if vnodes_len < 10 {
                    let nodes = VNODES.lock().await; 
                    for node in nodes.iter() { 
                        let mut node_guard = node.lock().await; 
                        if node_guard.version == 0 {
                            debug!("[*] Skipping ADDR message to node {:?}: version not set", node_guard.addr.address.to_string());
                            continue; // Skip nodes that haven't completed version handshake
                        }
                        node_guard.push_message(COMMANDS::ADDR, ()).await; // Queue an ADDR message to request peer addresses
                    }
                } else {
                    debug!("[*] Skipping peer address request: already have {} peers", vnodes_len); 
                }
            }
        }
    }
}


pub async fn start_node(data_dir: Option<&str>, mode: Option<SocketMode>, _remote_addr: Option<String>) {
    // Start the node with optional data directory and mode
    println!("[*] Starting node");

    let data_dir = data_dir.unwrap_or(DEFAULT_NODE_CONFIG::DATA_DIR);
    let mode = mode.unwrap_or(DEFAULT_NODE_CONFIG::MODE);
    let debug_log = format!("{}/debug.log", data_dir);

    // Create standard socket
    let socket = match Socket::new_standard(data_dir, mode).await {
        Some(s) => s,
        None => {
            println!("Failed to create standard socket; aborting");
            return;
        }
    };

    init_logging(&debug_log);

    // Spawn listening task
    let mut listen_socket = socket.clone();
    tokio::spawn(async move {
        listen_socket.listen().await;
    });

    let p_socket = Arc::new(Mutex::new(socket));
    *SOCKET.lock().await = Some(p_socket.clone());

    let addr = p_socket.lock().await.getsockaddr().await.expect("Failed to get sock address");

    *LOCAL_ADDR.lock().await = Some(Arc::new(Mutex::new(Address {
        address: addr.clone(),
        services: 0,
        last_failed_time: 0,
    })));

    println!("[*] Using data directory: {}", data_dir);
    println!("[*] Running in mode: {:?}", mode);
    println!("[*] Address: {:?}", addr);

    info!("[*] Starting node");
    info!("[*] Using data directory: {}", data_dir);
    info!("[*] Running in mode: {:?}", mode);
    info!("[*] Address: {:?}", addr);

    // Load peer addresses from file
    match load_addresses("addresses.txt").await {
        Ok(()) => {
            let addresses = VADDRESSES.lock().await;
            println!("[*] Loaded {} addresses into VADDRESSES", addresses.len());
        }
        Err(e) => {
            println!("[ERROR] Failed to load addresses: {}", e);
            return;
        }
    }

    // Setup stop signal channel
    let (tx, mut rx) = broadcast::channel(1);
    {
        let mut stop_signal = STOP_SIGNAL.lock().await;
        *stop_signal = Some(tx);
    }

    // Spawn background tasks for connections and message handling
    tokio::spawn({ async move { open_connections().await; } });
    tokio::spawn({ async move { send_messages().await; } });
    tokio::spawn({ async move { handle_messages().await; } });
    tokio::spawn({ async move { process_messages().await; } });
    tokio::spawn({ async move { node_manager().await; } });
    
    tokio::select! {
        _ = signal::ctrl_c() => { stop_node().await; }
        _ = rx.recv() => { stop_node().await; }
    }
}

pub async fn stop_node() {
    // Stop node and cleanup
    println!("[*] Stopping node...");



    // STOP signal 
    if let Some(signal) = STOP_SIGNAL.lock().await.as_ref() {
        let _ = signal.send(true);
    }


    // Disconnect the socket 
    if let Some(socket) = SOCKET.lock().await.as_ref().cloned() {
        socket.lock().await.disconnect().await;
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Clear node lists and socket reference
    VNODES.lock().await.clear();
    VADDRESSES.lock().await.clear();
    *SOCKET.lock().await = None;

    println!("[*] Node stopped");
}
