P2P Over the Mixnet is ready for testing. It’s a peer-to-peer network inspired by Bitcoin’s architecture, but instead of plain sockets, it runs on top of the Nym mixnet. That means traffic can flow between identified and anonymous participants without breaking the mesh. Nodes know how to handshake  `version`/`verack`, exchange addresses `ADDR`/`GETADDR`, push messages, and expand the swarm on their own. 

Right now, two individual nodes can find each other, connect, and trade messages directly. The next step is even more interesting: anonymous nodes will be able to link up with individual ones, blurring the line between public and hidden presence. What you see today is a working core — message passing, peer exchange, resilient growth. What’s missing, pings and dead-peer detection, is coming next. The code is minimal nearly 1300 lines, as its use the [`nymlib`](https://github.com/valansai/nymlib/README.md) a Rust workspace that I publish to make it easier to build applications on top of the Nym mixnet, as its provides:
- `nymsocket`: A high level socket like abstraction of `nymsdk`
- `crypto`: A cryptographic library for key generation, encryption, decryption, signing, and verification using ECDSA, RSA, and ECDHE algorithms.
- `serialize`: A flexible serialization framework like Bitcoins for encoding and decoding data structures with support for compact size encoding and hash generation.
- `serialize_derive`: A procedural macro crate for generating serialization and deserialization code for structs



Here’s how to spin up a small swarm on your own machine. Four nodes, all linking up through the mesh. 

## How to install 
```bash
git clone https://github.com/valansai/peer-to-peer.git
cd peer-to-peer
cargo run
```

### Usage
```bash
target/debug/peer-to-peer --mode <type> --path <dir>
```
- --`mode` <type>: the node type. Currently only individual is supported. (Anonymous mode will arrive in the next release).
- --`path` <dir>: the directory where the node stores its Nym client configuration and its debug.log file.
- This ensures the node comes back online with the same address after a shutdown or restart, a `debug.log` file will also be created inside this directory (e.g. alice/debug.log), useful for troubleshooting or analyzing network behavior.
- --`mode` individual mode means other nodes learn your nym client address
- --`mode` anonymous means other nodes not learn even your nym client address
   
``` bash 
target/debug/peer-to-peer --mode individual --path alice
```
This starts a node in individual mode with data dir path alice, stores her Nym config under alice/, and writes logs to alice/debug.log.


### Test 4 Nodes, Fully Connected. 

Here’s how to spin up a small swarm on your own machine. Four nodes, all linking up through the mesh.

> > **Note:** This is experimental. While basic functionality works, you should expect bugs, glitches, or unexpected behavior. This is not an error-free network — use it for testing and exploration only. When you spin up a small, fully connected swarm on your local machine, you can directly observe the network in action. Each node discovers peers, exchanges addresses, and forms a mesh where every node can communicate with the others. Logs show handshakes, message propagation, and  peer connections happening in real time. While this is still a simplified and experimental setup, it clearly demonstrates the network’s core behavior: resilient growth, peer-to-peer message passing, and the ability for nodes to expand the swarm automatically.
>



1. **Start Alice (individual node):**
   ```bash
   target/debug/peer-to-peer --mode individual --path alice
   ```
Alice boots up and prints her node address. Copy it, and add it on file addresses.txt

2. **Start Bob (individual node):**
   ```bash
   target/debug/peer-to-peer --mode individual --path bob
   ```
   Bob automatically connects to Alice, as will read its address from `addresses.txt` file

3. **Start John (individual node):**
   ```bash
   target/debug/peer-to-peer --mode individual --path john
   ```
   John connects to Alice. Alice passes John Bob’s address, and soon John and Bob are directly linked as well.

4. **Start Vivian (individual node):**
   ```bash
   target/debug/peer-to-peer --mode individual --path vivian
   ```
   Vivian connects to Alice. Alice shares Bob and John’s addresses, so Vivian links with them too.



`Nym address:` n1cf9fy9wvcp04wdf993qw2fre606ujlxye0yry4
