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



use tokio::net::UnixListener;
use tokio::net::UnixStream;
use tokio::io::
{
    AsyncReadExt,
    AsyncWriteExt
};

use std::time::{SystemTime, UNIX_EPOCH};


pub async fn handle_control(mut stream: UnixStream) {
    let mut buf = [0u8; 1024];
    let n = match stream.read(&mut buf).await {
        Ok(n) if n == 0 => return,
        Ok(n) => n,
        Err(e) => {
            log::error!("Failed to read from control socket: {}", e);
            return;
        }
    };

    let cmd = String::from_utf8_lossy(&buf[0..n]).trim().to_string();
    let mut parts = cmd.splitn(2, ' ');
    let keyword = parts.next().unwrap_or_default();
    let arg = parts.next();

    match keyword {
        "stop" => {
            crate::network::stop_node().await;
            let _ = stream.write_all(b"stopping\n").await;
        },
        &_ => todo!()
    }
}