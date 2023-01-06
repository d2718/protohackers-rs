/*!
Protohackers Problem 1: Prime Time

Officials have devised a JSON-based request-response protocol. Each request
is a single line containing a JSON object, terminated by a newline character
('\n', or ASCII 10). Each request begets a response, which is also a single
line containing a JSON object, terminated by a newline character.

After connecting, a client may send multiple requests in a single session.
Each request should be handled in order.

A conforming request object has the required field method, which must always
contain the string "isPrime", and the required field number, which must
contain a number. Any JSON number is a valid number, including floating-point
values.

Example request:

```json
{"method":"isPrime","number":123}
```

Extraneous fields are to be ignored.

A conforming response object has the required field method, which must always
contain the string "isPrime", and the required field prime, which must
contain a boolean value: true if the number in the request was prime, false
if it was not.

Example response:

```json
{"method":"isPrime","prime":false}
```

Accept TCP connections.

Whenever you receive a conforming request, send back a correct response, and
wait for another request.

Whenever you receive a malformed request, send back a single malformed
response, and disconnect the client.

Make sure you can handle at least 5 simultaneous clients.
*/

use std::{
    io::ErrorKind,
    sync::Mutex,
};

use once_cell::sync::Lazy;
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use ph::primes::Primes;

static LOCAL_ADDR: &str = "0.0.0.0:12321";
const BUFFSIZE: usize = 1024;

static PRIMES: Lazy<Mutex<Primes>> = Lazy::new(||
    Mutex::new(Primes::default())
);

#[derive(Clone, Deserialize)]
struct FloatReq {
    method: String,
    #[allow(dead_code)]
    number: f64,
}

#[derive(Clone, Deserialize, Debug)]
struct IntReq {
    method: String,
    number: i64,
}

fn process_request(bytes: Vec<u8>) -> Result<bool, String> {

    let req: IntReq = match serde_json::from_slice(&bytes) {
        Ok(req) => req,
        Err(_) => match serde_json::from_slice::<FloatReq>(&bytes) {
            Ok(req) => {
                if &req.method == "isPrime" {
                    return Ok(false);
                } else {
                    return Err(format!("Unrecognized method: {:?}", &req.method));
                }
            },
            Err(e) => {
                return Err(format!("Request couldn't be deserialized: {}", &e));
            }
        },
    };

    if &req.method != "isPrime" {
        return Err(format!("Unrecognized method: {:?}", &req.method));
    }

    log::debug!("Rec'd request: {:?}", &req);

    // To be able to safely cast to u64, we need to ensure it's positive.
    // Also, no primes are less than 2 anyway.
    if req.number < 2 { return Ok(false); }

    let is_prime = PRIMES.lock().unwrap().is_prime(req.number as u64);
    log::debug!("{} ? {}", req.number, is_prime);
    Ok(is_prime)
}

async fn copy_and_process(
    from: &[u8],
    mut buff: Vec<u8>,
    sock: &mut TcpStream,
) -> Result <Vec<u8>, String> {
    for b in from.iter() {
        let b = *b;
        if b == b'\n' {
            let mut req_buff: Vec<u8> = Vec::new();
            std::mem::swap(&mut buff, &mut req_buff);
            let resp = match process_request(req_buff) {
                Ok(is_prime) => {
                    format!(
                        r#"{{"method":"isPrime","prime":{}}}
"#, is_prime
                    )
                },
                Err(e) => {
                    log::warn!("{}", &e);
                    String::from("{{}}\n")
                },
            };
            sock.write_all(resp.as_bytes()).await.map_err(|e| format!(
                "Error writing response: {}", &e
            ))?;
        } else {
            buff.push(b);
        }
    }

    Ok(buff)
}

async fn handle(mut sock: TcpStream, client_n: usize) -> usize {
    let mut readbuff = [0u8; BUFFSIZE];

    let mut buff: Vec<u8> = Vec::new();

    loop {
        let res = sock.read(&mut readbuff).await;
        match res {
            Ok(0) => { break; },
            Ok(n) => {
                let res = copy_and_process(&readbuff[..n], buff, &mut sock).await;
                match res {
                    Ok(new_buff) => { buff = new_buff; },
                    Err(e) => {
                        log::warn!("{}", &e);
                        break;
                    }
                }
            },
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    continue;
                } else {
                    log::error!("Error reading from socket: {}", &e);
                    break;
                }
            }
        }
    }

    if let Err(e) = sock.shutdown().await {
        eprintln!("Error shutting down socket: {}", &e);
    }

    client_n
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    let listener = TcpListener::bind(LOCAL_ADDR).await.unwrap();
    log::info!("Bound do {}", LOCAL_ADDR);

    let mut client_n: usize = 0;

    loop {
        match listener.accept().await {
            Ok((sock, addr)) => {
                println!("Accepted #{} from {:?}", client_n, &addr);
                tokio::spawn(async move {
                    handle(sock, client_n).await
                });
                client_n += 1;
            },
            Err(e) => {
                eprintln!("Error with incoming connection: {}", &e);
            },
        }
    }
}