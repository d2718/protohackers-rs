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

/// Messages from the `Room` to `Client`s.
#[derive(Clone, Debug)]
enum Msg {
    /// Deliver to every user but `id`.
    All{ id: usize, text: String },
    /// Deliver to only user `id`.
    One{ id: usize, text: String },
}

/// `Client` actions to report to the `Room`.
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

    /// Generate a message listing all the current occupants.
    fn name_list(&self) -> String {
        let names: Vec<&str> = self.users.iter()
            .map(|(_, name)| name.as_str())
            .collect();
        
        format!("* Also here: {}\n", &names.join(", "))
    }

    /// Run the room.\
    /// 
    /// There is a lot of `.unwrap()`ping going on here, but
    ///   * If `self.blow.send()` returns an error, we have serious problems,
    ///     so let's just die.
    ///   * The requested key should _always_ be in `self.users`; if it's
    ///     not, something way weird has happened, so again, let's die.
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
                    // If no one is left in the `Room`, this will return an
                    // error, so we are satisfy the compiler here by
                    // "handling" it.
                    let _ = self.blow.send(msg);
                },
            }
        }
    }
}

/// Handles to a connected client's socket, internal buffer, and user id.
struct Client {
    id: usize,
    suck: BufReader<ReadHalf<TcpStream>>,
    blow: WriteHalf<TcpStream>,
    buff: Vec<u8>,
}

// Possible results of calling `Client::get_line()`.
enum ClientResult {
    Line(String),
    Eof,
    Err(String),
}

impl Client {
    pub fn new(sock: TcpStream, id: usize) -> Client {
        let (r, w) = tokio::io::split(sock);
        Client {
            id,
            suck: BufReader::new(r),
            blow: w,
            buff: Vec::new(),
        }
    }

    /// Attempt to read a single line of text from the socket.
    pub async fn get_line(&mut self) -> ClientResult {
        let res = self.suck.read_until(b'\n', &mut self.buff).await;
        log::debug!("Client {} read_line() result: {:?}", self.id, &res);
        match res {
            Ok(0) => ClientResult::Eof,
            Ok(_) => {
                let mut new_buff: Vec<u8> = Vec::new();
                std::mem::swap(&mut self.buff, &mut new_buff);

                // The spec says that all incoming text should be ASCII, but
                // we're going to be defensive here anyway.
                let mut line: String = match String::from_utf8(new_buff) {
                    Ok(line) => line,
                    Err(e) => {
                        log::warn!(
                            "Client {} rec'd non-UTF-8 input; returning approximation.",
                            self.id
                        );
                        // We need the `.into()` because this function
                        // returns a `Cow`, and we want to be sure we have
                        // a `String`.
                        String::from_utf8_lossy(&e.into_bytes()).into()
                    }
                };

                // This unwrapping is okay because we've read at least one
                // byte into `self.buff`.
                if *line.as_bytes().last().unwrap() != b'\n' {
                    // This might happen if this is the last line from the
                    // client. We'll add the newline because the function of
                    // the rest of the program depends on it.
                    line.push('\n');
                }
                log::debug!("Client {} read_line() returns {:?}", self.id, &line);
                ClientResult::Line(line)
            },
            Err(e) => ClientResult::Err(format!("{}", &e)),
        }
    }

    /// Attempt to write a message to the socket.
    pub async fn write(&mut self, chunk: &[u8]) -> Result<(), ()> {
        log::trace!(
            "Client {} attempting to write: {:?}",
            self.id, String::from_utf8_lossy(chunk)
        );
        if let Err(e) = self.blow.write_all(chunk).await {
            log::error!(
                "Client {}: error writing to socket: {}", self.id, &e
            );
            Err(())
        } else {
            Ok(())
        }
    }

    pub async fn shutdown(self) {
        let mut sock = self.suck.into_inner().unsplit(self.blow);
        if let Err(e) = sock.shutdown().await {
            log::error!("Client {}: error shutting down socket: {}", self.id, &e);
        }
        log::info!("Client {} disconnects.", self.id);
    }
}

/// Ensure a new client's name conforms to the requirements: A nonzero number
/// of only alphanumeric characters.
fn name_ok(name: &str) -> bool {
    if name.is_empty() { return false; }

    for c in name.chars() {
        if !c.is_alphanumeric() { return false; }
    }

    return true;
}

/// Interact with the client.
/// 
/// This should be run in its own async task.
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
        // This is kind of a hack, but I can't think of better way to do this
        // that isn't unnecessarily labyrinthine.
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