mod net;
mod net_addr;
mod net_msg_header;
mod node;
mod util;


use nymlib::nymsocket::{SocketMode, SockAddr};
use clap::Parser;


#[derive(Parser, Debug)]
#[command(name = "nymp2p", about = "Runs a Nym P2P node on the mixnet")]
struct Cli {
    #[arg(long, help = "Client mode: individual or anonymous", value_enum)]
    mode: SocketMode,

    #[arg(long, help = "Client path for the mixnet")]
    path: Option<String>,

    #[arg(long, help = "Remote node addr to connect")]
    addr: Option<String>,
}





#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    net::start_node(cli.path.as_deref(), Some(cli.mode), cli.addr).await;

}