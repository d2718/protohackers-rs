/*!
Protohackers Problem 3: Budget Chat

Implement the [Budget Chat protocol](https://protohackers.com/problem/3).
*/

use std::collections::BTreeMap;

use tokio::{
    io::{
        AsyncWriteExt, BufReader, AsyncBufReadExt,
        ReadHalf, WriteHalf,
    },
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc},
};

const LOCAL_ADDR: &str = "0.0.0.0:12321";
const EVT_CHANNEL_SIZE: usize = 256;
const BCAST_CHANNEL_SIZE: usize = 256;
const LAGGED_TEXT: &[u8] = b"Your connection has lagged and dropped messages.\n";
const WELCOME_TEXT: &[u8] = b"Welcome. Please enter the name you'd like to use.\n";
const REJECT_TEXT: &[u8] = b"Your name must consist of one or more alphanumeric characters.\n";

#[derive(Clone, Debug)]
enum Msg {
    /// Deliver to every user but `id`.
    All{ id: usize, text: String },
    /// Deliver to only user `id`.
    One{ id: usize, text: String },
}

#[derive(Clone, Debug)]
enum Evt {
    Text{ id: usize, text: String },
    Leave(usize),
    Arrive{ id: usize, name: String },
}

struct Room {
    users: BTreeMap<usize, String>,
    suck: mpsc::Receiver<Evt>,
    blow: broadcast::Sender<Msg>,
}

impl Room {
    pub fn new(
        evt_chan: mpsc::Receiver<Evt>,
        bcast_chan: broadcast::Sender<Msg>,
    ) -> Self {
        Self {
            users: BTreeMap::new(),
            suck: evt_chan,
            blow: bcast_chan,
        }
    }

    fn name_list(&self) -> String {
        let names: Vec<&str> = self.users.iter()
            .map(|(_, name)| name.as_str())
            .collect();
        
        format!("* Also here: {}\n", &names.join(", "))
    }

    pub async fn run(&mut self) {
        while let Some(evt) = self.suck.recv().await {
            log::info!("room: {:?}", &evt);
            match evt {
                Evt::Text { id, text } => {
                    let name = self.users.get(&id).unwrap();
                    let text = format!("[{}] {}", name, &text);
                    self.blow.send(Msg::All{ id, text }).unwrap();
                },
                Evt::Arrive{ id, name } => {
                    let msg = Msg::All {
                        text: format!("* {} joins.\n", &name),
                        id: id,
                    };
                    self.blow.send(msg).unwrap();

                    let msg = Msg::One {
                        text: self.name_list(),
                        id: id,
                    };
                    self.blow.send(msg).unwrap();
                    self.users.insert(id, name);
                },
                Evt::Leave(id) => {
                    let name = self.users.remove(&id).unwrap();
                    let msg = Msg::All {
                        text: format!("* {} leaves.", &name),
                        id: id,
                    };
                    let _ = self.blow.send(msg);
                },
            }
        }
    }
}

struct Client {
    id: usize,
    sndr: WriteHalf<TcpStream>,
    rcvr: BufReader<ReadHalf<TcpStream>>,
}

impl Into<TcpStream> for Client {
    fn into(self) -> TcpStream {
        self.rcvr.into_inner().unsplit(self.sndr)
    }
}

enum ClientResult {
    Line(String),
    Last(String),
    Eof,
    Err(String),
}

impl Client {
    pub fn new(s: TcpStream, id: usize) -> Client {
        let (rh, wh) = tokio::io::split(s);
        Client {
            id,
            sndr: wh,
            rcvr: BufReader::new(rh),
        }
    }

    pub async fn get_line(&mut self) -> ClientResult {
        let mut buff = String::new();
        let res = self.rcvr.read_line(&mut buff).await;
        log::debug!("Client {} read_line() result: {:?}", self.id, &res);
        match res {
            Ok(0) => ClientResult::Eof,
            Ok(_) => {
                if *buff.as_bytes().last().unwrap() == b'\n' {
                    log::debug!("Client {} read_line() returns {:?}", self.id, &buff);
                    ClientResult::Line(buff)
                } else {
                    log::debug!("Client {} read_line() returns {:?}", self.id, &buff);
                    ClientResult::Last(buff)
                }
            },
            Err(e) => ClientResult::Err(format!("{}", &e)),
        }
    }

    pub async fn write(&mut self, chunk: &[u8]) -> Result<(), ()> {
        log::trace!(
            "Client {} attempting to write {:?}",
            self.id, String::from_utf8_lossy(chunk)
        );
        if let Err(e) = self.sndr.write_all(chunk).await {
            log::error!(
                "Client {}: error writing to socket: {}", self.id, &e
            );
            Err(())
        } else {
            Ok(())
        }
    }

    pub async fn shutdown(self) {
        let mut sock: TcpStream = self.rcvr.into_inner().unsplit(self.sndr);
        if let Err(e) = sock.shutdown().await {
            log::error!("Client {}: error shutting down socket: {}", self.id, &e);
        }
        log::info!("Client {} disconnects.", self.id);
    }
}

fn name_ok(name: &str) -> bool {
    if name.is_empty() { return false; }

    for c in name.chars() {
        if !c.is_alphanumeric() { return false; }
    }

    return true;
}

async fn run_client(
    mut client: Client,
    mut recv: broadcast::Receiver<Msg>,
    send: mpsc::Sender<Evt>
) {
    if client.write(WELCOME_TEXT).await.is_err() {
        client.shutdown().await;
        return;
    }
    
    if let ClientResult::Line(name) = client.get_line().await {
        let name = name.trim().to_string();
        if !name_ok(&name) {
            log::info!("Client {} attempts bad name: {:?}", client.id, &name);
            let _ = client.write(REJECT_TEXT).await;
            client.shutdown().await;
            return;
        }

        // Empty the channel, in case any messages leaked in prior to the join.
        while let Ok(_) = recv.try_recv() { /* do bupkis */ }

        let evt = Evt::Arrive{ id: client.id, name };
        send.send(evt).await.unwrap();
    } else {
        log::error!("Error receiving a name message from Client {}.", client.id);
        client.shutdown().await;
        return;
    }

    loop {
        tokio::select!{
            res = client.get_line() => match res {
                ClientResult::Line(line) => {
                    let evt = Evt::Text{ id: client.id, text: line };
                    send.send(evt).await.unwrap();
                },
                ClientResult::Last(mut line) => {
                    line.push('\n');
                    let evt = Evt::Text{ id: client.id, text: line };
                    send.send(evt).await.unwrap();
                    break;
                },
                ClientResult::Eof => { break; },
                ClientResult::Err(e) => {
                    log::error!("Error reading from client {} socket: {}", client.id, &e);
                    break;
                }
            },
            res = recv.recv() => {
                log::info!("Client {}: {:?}", client.id, &res);
                match res {
                    Ok(Msg::All{ id, text }) => {
                        if id != client.id {
                            if client.write(text.as_bytes()).await.is_err() { break; }
                        }
                    },
                    Ok(Msg::One{ id, text }) => {
                        if id == client.id {
                            if client.write(text.as_bytes()).await.is_err() { break; }
                        }
                    },
                    Err(broadcast::error::RecvError::Closed) => {
                        log::error!("Broadcast channel closed.");
                        break;
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        log::warn!("Client {} has dropped messages.", client.id);
                        if client.write(LAGGED_TEXT).await.is_err() { break; }
                    },
                }
            }
        }
    }

    send.send(Evt::Leave(client.id)).await.unwrap();
    client.shutdown().await;
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    let (evt_tx, evt_rx) = mpsc::channel(EVT_CHANNEL_SIZE);
    let (bcast_tx, _) = broadcast::channel(BCAST_CHANNEL_SIZE);
    let mut room = Room::new(evt_rx, bcast_tx.clone());
    tokio::spawn(async move { room.run().await; });
    let listener = TcpListener::bind(LOCAL_ADDR).await.unwrap();
    log::info!("Bound to {}", LOCAL_ADDR);

    let mut client_n: usize = 0;
    loop {
        match listener.accept().await {
            Ok((sock, addr)) => {
                log::info!("Rec'd connection {} from {:?}", client_n, &addr);
                let client = Client::new(sock, client_n);
                client_n += 1;
                let (bcast_tr, evt_tx) = (bcast_tx.subscribe(), evt_tx.clone());
                tokio::spawn(async move { 
                    run_client(client, bcast_tr, evt_tx).await;
                });
            },
            Err(e) => {
                log::error!("Error with incoming connection: {}", &e);
            }
        }
    }
}