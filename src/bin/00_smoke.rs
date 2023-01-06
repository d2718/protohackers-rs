/*!
Protohackers Problem 0: Smoke Test

Implement the TCP Echo Service; be able to handle at least 5 simultaneous
clients.
*/
use std::io::ErrorKind;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

static LOCAL_ADDR: &str = "0.0.0.0:12321";
const BUFFSIZE: usize = 1024;

async fn handle(mut sock: TcpStream) {
    let mut buff = [0u8; BUFFSIZE];

    sock.readable().await.unwrap();

    loop {
        match sock.read(&mut buff).await {
            Ok(0) => { break; }
            Ok(n) => {
                println!("Read {} bytes", n);
                sock.write_all(&buff[..n]).await.unwrap();
                println!("Finished writing.");
            },
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    panic!("Oops! {}", &e);
                } else {
                    continue;
                }
            }
        }
    }
    println!("Dropping connection.");
    sock.shutdown().await.unwrap();
}

 #[tokio::main(flavor = "current_thread")]
 async fn main() {
    let listener = TcpListener::bind(LOCAL_ADDR).await.unwrap();
    println!("Bound to {}", LOCAL_ADDR);

    loop {
        match listener.accept().await {
            Ok((socket, addr))  => {
                println!("Accepted incoming from {:?}", &addr);
                handle(socket).await;
            },
            Err(e) => {
                println!("Error with incoming connection: {}", &e);
            }
        }
    }

 }