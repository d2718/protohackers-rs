/*!
Protohackers Problem 1: Tracking Prices

Collect timestamped price messages from each client and supply averages
over given ranges.
*/
use std::{
    collections::BTreeMap,
    io::ErrorKind,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const LOCAL_ADDR: &str = "0.0.0.0:12321";

#[derive(Debug, Clone, Copy)]
struct Insert {
    timestamp: i32,
    price: i32,
}

#[derive(Debug, Clone, Copy)]
struct Query {
    begin: i32,
    end: i32,
}

#[derive(Debug, Clone, Copy)]
enum Msg {
    I(Insert),
    Q(Query),
}

fn range_average(map: &BTreeMap<i32, i32>, low: i32, high: i32) -> i32 {
    if high < low { return 0; }

    let mut tot: i64 = 0;
    let mut n: i64 = 0;

    for (&ts, &val) in map.iter() {
        if ts < low {
            continue;
        } else if ts <= high {
            tot += val as i64;
            n += 1 as i64;
        } else {
            break;
        }
    }

    if n == 0 { return 0; }
    (tot / n) as i32
}

impl Msg {
    pub fn decode(data: &[u8; 9]) -> Result<Msg, String> {
        let mut buff = [0u8; 4];
        
        buff.clone_from_slice(&data[1..5]);
        let a = i32::from_be_bytes(buff);
        let mut buff = [0u8; 4];
        buff.clone_from_slice(&data[5..9]);
        let b = i32::from_be_bytes(buff);

        match data[0] {
            b'I' => { Ok(
                Msg::I(Insert{ timestamp: a, price: b })
            ) },
            b'Q' => { Ok(
                Msg::Q(Query{ begin: a, end: b })
            ) },
            x => { Err(format!(
                "unrecognized behavior: {} (expected {} or {})",
                x, b'I', b'Q'
            ))}
        }
    }
}

async fn handler(sock: &mut TcpStream) -> Result<(), String> {
    let mut buff = [0u8; 9];
    let mut prices: BTreeMap<i32, i32> = BTreeMap::new();

    loop {
        if let Err(e) = sock.read_exact(&mut buff).await {
            if e.kind() == ErrorKind::UnexpectedEof {
                return Ok(());
            } else {
                return Err(format!(
                    "Error reading from socket: {}", &e
                ));
            }
        }

        match Msg::decode(&buff)? {
            Msg::I(m) => {
                prices.insert(m.timestamp, m.price);
            },
            Msg::Q(m) => {
                let avg = range_average(&prices, m.begin, m.end);
                let buff = avg.to_be_bytes();
                sock.write_all(&buff).await.map_err(|e| format!(
                    "Error writing response to socket: {}", &e
                ))?;
            },
        }
    }
}

async fn handler_wrapper(mut sock: TcpStream, client_n: usize) {
    if let Err(e) = handler(&mut sock).await {
        log::info!("Error handling client {}: {}", client_n, &e);
    }
    match sock.shutdown().await {
        Ok(()) => {
            log::info!("Disconnected from client {}.", client_n);
        },
        Err(e) => {
            log::error!(
                "Error shutting down socket from client {}: {}",
                client_n, &e
            );
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    let listener = TcpListener::bind(LOCAL_ADDR).await.unwrap();
    log::info!("Bound to {}", LOCAL_ADDR);

    let mut client_n: usize = 0;

    loop {
        match listener.accept().await {
            Ok((sock, addr)) => {
                log::info!("Accepted client {} from {:?}", client_n, &addr);
                tokio::spawn(async move {
                    handler_wrapper(sock, client_n).await
                });
                client_n += 1;
            },
            Err(e) => {
                log::error!("Error with incoming connection: {}", &e);
            },
        }
    }
}