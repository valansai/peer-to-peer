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


use clap::
{
    Parser, 
    Subcommand
};

use tokio::io::
{
    AsyncReadExt, 
    AsyncWriteExt
};

use tokio::net::UnixStream;




use node::network;
use nymlib::nymsocket::SocketMode;




#[derive(Parser, Debug)]
#[command(name = "NyxNet", about = "Runs a NyxNet node")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(long, default_value = "NyxNet", help = "Data directory for the node", global = true)]
    datadir: String,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Start the node")]
    Start {
        #[arg(long, help = "Client mode: individual or anonymous", value_enum)]
        mode: SocketMode,
    },

    #[command(about = "Stop the node")]
    Stop,

    #[command(about = "Check node status")]
    Status,
}


#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let control_path = format!("{}/control.sock", cli.datadir);

    match cli.command {
        Command::Start { mode } => {
            if UnixStream::connect(&control_path).await.is_ok() {
                println!("Node is already running");
                return;
            }
            crate::network::start_node(Some(&cli.datadir), Some(mode)).await;
        }
        Command::Stop => {
            let mut stream = match UnixStream::connect(&control_path).await {
                Ok(s) => s,
                Err(_) => {
                    println!("Node is not running");
                    return;
                }
            };
            if let Err(e) = stream.write_all(b"stop\n").await {
                println!("Failed to send stop command: {}", e);
                return;
            }
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
        }

        Command::Status => {
            match UnixStream::connect(&control_path).await {
                Ok(_) => println!("Node is running"),
                Err(_) => println!("Node is not running"),
            }
        }
    }
}

