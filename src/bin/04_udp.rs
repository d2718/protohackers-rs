/*!
Protohackers Problem 04: Unusual Database Program

*/
use std::collections::HashMap;
use tokio::net::UdpSocket;

static LOCAL_ADDR: &str = "0.0.0.0:12321";
const BUFFSIZE: usize = 1024;
static VERSION_REQUEST: &[u8] = b"version";
static VERSION: &[u8] = b"version=Ken's Key-Value Store v -0.1";

async fn run(
    sock: &UdpSocket,
    buff: &mut [u8; BUFFSIZE],
    db: &mut HashMap<Vec<u8>, Vec<u8>>
) -> std::io::Result<()> {
    log::trace!("run() called");

    loop {
        let (len, addr) = sock.recv_from(buff).await?;
        let data = &buff[..len];
        log::debug!("rec'd {} bytes: {:?}", len, &String::from_utf8_lossy(data));

        if data == VERSION_REQUEST {
            sock.send_to(VERSION, addr).await?;
            log::debug!("Sent VERSION message.");
            continue;
        }

        if let Some(n) = data.iter().position(|&b| b == b'=') {
            if &data[..n] == VERSION_REQUEST {
                // Let this packet hit the floooor.
                continue;
            }

            let key = Vec::from(&data[..n]);
            let val = Vec::from(&data[(n+1)..]);

            log::debug!(
                "Inserting {:?}={:?}.",
                &String::from_utf8_lossy(&key),
                &String::from_utf8_lossy(&val)
            );

            db.insert(key, val);

        } else {
            if let Some(val) = db.get(data) {
                let length = val.len() + data.len() + 1;
                let mut response = Vec::with_capacity(length);

                response.extend_from_slice(data);
                response.push(b'=');
                response.extend_from_slice(val);

                log::debug!(
                    "Sending response: {}",
                    &String::from_utf8_lossy(&response)
                );

                sock.send_to(&response, addr).await?;
            }
            // Otherwise, we just drop it on the floor.
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    let sock = UdpSocket::bind(LOCAL_ADDR).await.unwrap();
    log::info!("Listening on {:?}", LOCAL_ADDR);
    let mut buff =[0u8; 1024];
    let mut db: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();

    loop {
        if let Err(e) = run(&sock, &mut buff, &mut db).await {
            log::error!("{}", &e);
        }
    }
}