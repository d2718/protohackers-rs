/*!
Protohackers Problem 05: Stealing Boguscoin for the Mob

Proxy upstream Budget Chat at `chat.protohackers.com:16963`
to change all boguscoin addresses to Tony's.

Tony's address: 7YWHMfk9JZe0LM0g1ZauHuiSxhI

A valid boguscoin address satsfies all of the following:

  * it starts with a "7"
  * it consists of at least 26, and at most 35, alphanumeric characters
  * it starts at the start of a chat message, or is preceded by a space
  * it ends at the end of a chat message, or is followed by a space
*/
use std::ops::Range;

use lua_patterns::LuaPattern;
use tokio::{
    io::{
        AsyncWriteExt, BufReader, AsyncBufReadExt,
        ReadHalf, WriteHalf,
    },
    net::{TcpListener, TcpStream},
};

static VERSION: &str = "3";

static LOCAL_ADDR: &str = "0.0.0.0:12321";
static SERVER_ADDR: &str = "chat.protohackers.com:16963";
static TONYS_BC_ADDR: &[u8] = b"7YWHMfk9JZe0LM0g1ZauHuiSxhI";
static BC_PATT: &str = "7[A-Za-z0-9]+";
const BUFF_CAPACITY: usize = 1024;

fn match_is_good(buff: &[u8], start: usize, end: usize) -> bool {
    let length = end - start;
    if length < 26 || length > 35 { return false; }

    if start > 0 {
        if buff[start-1] != b' ' {
            return false;
        }
    }

    if end < buff.len() - 1 {
        if buff[end] != b' ' && buff[end] != b'\n' {
            return false
        }
    }

    true
}

/// Scan a message, starting at index `start`, for a boguscoin address,
/// and return its span if found.
fn find_address(
    buff: &[u8],
    patt: &mut LuaPattern,
    start: usize,
) -> Option<Range<usize>> {
    if patt.matches_bytes(&buff[start..]) {
        let r = patt.range();
        let r = Range{ start: r.start + start, end: r.end + start };

        if match_is_good(buff, r.start, r.end) {
            return Some(r)
        } else {
            return find_address(buff, patt, r.end);
        }
    }
    
    None
}

/// Return a vector copy of the supplied `buff` with all boguscoin addresses
/// replaced with Tony's.
///
/// Should call find_address(...) on the buffer first to check for at least
/// _one_ BCoin address (and supply the returned range), otherwise you're
/// just copying the buffer for nothing.
fn substitute_addresses(
    buff: &[u8],
    patt: &mut LuaPattern,
    first_range: Range<usize>
) -> Vec<u8> {
    log::trace!(
        "substitute_addresses()\nbuff: {:?}",
        &String::from_utf8_lossy(buff)
    );

    let mut msg: Vec<u8> = Vec::with_capacity(BUFF_CAPACITY);
    let mut src_idx: usize = 0;

    let mut cur_r = Some(first_range);

    while let Some(r) = cur_r {
        log::trace!("    range: {:?}", &r);
        msg.extend_from_slice(&buff[src_idx..r.start]);
        msg.extend_from_slice(TONYS_BC_ADDR);
        
        log::trace!("    msg: {:?}", &String::from_utf8_lossy(&msg));

        src_idx = r.end;
        cur_r = find_address(buff, patt, src_idx);
    }

    msg.extend_from_slice(&buff[src_idx..]);

    log::trace!("final msg: {:?}", &String::from_utf8_lossy(&msg));

    msg
}

/// Possible results of calling `Filter::read_line()`.
enum RlResult {
    Line(Vec<u8>),
    Eof,
    Err(String)
}

struct Filter {
    id: usize,
    c2s_suck: BufReader<ReadHalf<TcpStream>>,
    c2s_blow: WriteHalf<TcpStream>,
    s2c_suck: BufReader<ReadHalf<TcpStream>>,
    s2c_blow: WriteHalf<TcpStream>,
    c2s_buff: Vec<u8>,
    s2c_buff: Vec<u8>,
}

impl Filter {
    pub fn new(
        id: usize,
        client_sock: TcpStream,
        server_sock: TcpStream,
    ) -> Filter {
        let (c2s_suck, s2c_blow) = tokio::io::split(client_sock);
        let (s2c_suck, c2s_blow) = tokio::io::split(server_sock);
        let c2s_suck = BufReader::new(c2s_suck);
        let s2c_suck = BufReader::new(s2c_suck);

        Filter {
            id, c2s_suck, c2s_blow, s2c_suck, s2c_blow,
            c2s_buff: Vec::new(),
            s2c_buff: Vec::new(),
        }
    }

    /// Attempt to read through the next newline from the given `reader`
    /// into `buff`.
    async fn read_line(
        reader: &mut BufReader<ReadHalf<TcpStream>>,
        buff: &mut Vec<u8>
    ) -> RlResult {
        match reader.read_until(b'\n', buff).await {
            Ok(0) => RlResult::Eof,
            Ok(_) => {
                let mut new_buff: Vec<u8> = Vec::new();
                std::mem::swap(buff, &mut new_buff);
                RlResult::Line(new_buff)
            },
            Err(e) => RlResult::Err(format!("{}", &e)),
        }
    }

    async fn write_line(
        writer: &mut WriteHalf<TcpStream>,
        buff: &[u8]
    ) -> Result<(), String> {
        writer.write_all(buff).await.map_err(|e| format!(
            "error writing message {:?}: {}",
            &String::from_utf8_lossy(buff), &e
        ))?;
        log::trace!("message written: {:?}",&String::from_utf8_lossy(buff));
        writer.flush().await.map_err(|e| format!(
            "error flushing socket: {}", &e
        ))?;
        log::trace!("...socket flushed.");

        Ok(())
    }

    pub async fn shutdown(self) {
        let mut client_sock = self.c2s_suck.into_inner()
                                    .unsplit(self.s2c_blow);
        let mut server_sock = self.s2c_suck.into_inner()
                                    .unsplit(self.c2s_blow);
        if let Err(e) = client_sock.shutdown().await {
            log::warn!(
                "Client {}: error shutting down client socket: {}",
                self.id, &e
            );
        }
        if let Err(e) = server_sock.shutdown().await {
            log::warn!(
                "Client {}: error shutting down server socket: {}",
                self.id, &e
            );
        }
        log::info!("Client {} disconnected.", self.id);
    }

    async fn welcome_handshake(&mut self) -> Result<String, String> {
        log::trace!(
            "Client {}: negotiating handshake.", self.id
        );

        let welcome_msg = match Filter::read_line(
            &mut self.s2c_suck,
            &mut self.s2c_buff
        ).await {
            RlResult::Line(msg) => msg,
            RlResult::Eof => {
                return Err("server failed to send welcome message.".into());
            },
            RlResult::Err(e) => {
                return Err(format!(
                    "error reading welcome message from server: {}", &e
                ));
            },
        };
        log::debug!("welcome message: {:?}", &String::from_utf8_lossy(&welcome_msg));

        Filter::write_line(&mut self.s2c_blow, &welcome_msg).await?;

        let name_msg = match Filter::read_line(
            &mut self.c2s_suck,
            &mut self.c2s_buff
        ).await {
            RlResult::Line(msg) => msg,
            RlResult::Eof => {
                log::debug!(
                    "Client {} c2s_buff: {:?}",
                    self.id, String::from_utf8_lossy(&self.c2s_buff)
                );
                return Err("encountered EOF reading name message from client.".into());
            },
            RlResult::Err(e) => {
                return Err(format!(
                    "error reading name message from client: {}", &e
                ));
            }
        };

        let name = String::from_utf8_lossy(&name_msg);
        let name = String::from(name.trim());

        Filter::write_line(&mut self.c2s_blow, &name_msg).await?;

        Ok(name)
    }

    async fn run(&mut self) -> Result<(), String> {
        log::trace!("Client {} running.", self.id);

        // Negotiate welcome/name handshake; save client's name for logging.
        let name = self.welcome_handshake().await?;

        let mut patt = LuaPattern::new(BC_PATT);

        loop {
            tokio::select!{
                res = Filter::read_line(
                    &mut self.s2c_suck,
                    &mut self.s2c_buff,
                ) => match res {
                    RlResult::Line(line) => {
                        log::trace!(
                            "Client {} ({}) <- server: {}",
                            self.id, &name, &String::from_utf8_lossy(&line)
                        );

                        let msg = match find_address(&line, &mut patt, 0) {
                            Some(rng) => substitute_addresses(&line, &mut patt, rng),
                            None => line,
                        };
                        Filter::write_line(&mut self.s2c_blow, &msg).await
                            .map_err(|e| format!("client socket: {}", &e))?;
                    },
                    RlResult::Eof => {
                        log::trace!(
                            "Client {} ({}) rec'd EOF from server.",
                            self.id, &name
                        );
                        break;
                    },
                    RlResult::Err(e) => {
                        return Err(format!(
                            "error reading from server socket: {}", &e
                        ));
                    }
                },
                res = Filter::read_line(
                    &mut self.c2s_suck,
                    &mut self.c2s_buff,
                ) => match res {
                    RlResult::Line(line) => {
                        log::trace!(
                            "Client {} ({}) <- client: {}",
                            self.id, &name, &String::from_utf8_lossy(&line)
                        );

                        let msg = match find_address(&line, &mut patt, 0) {
                            Some(rng) => substitute_addresses(&line, &mut patt, rng),
                            None => line,
                        };
                        Filter::write_line(&mut self.c2s_blow, &msg).await
                            .map_err(|e| format!("server socket: {}", &e))?;
                    },
                    RlResult::Eof => {
                        log::trace!(
                            "Client {} ({}) rec'd EOF from client.", self.id, &name
                        );
                        break;
                    },
                    RlResult::Err(e) => {
                        return Err(format!(
                            "error reading from client socket: {}", &e
                        ));
                    }
                },
            }
        }

        Ok(())
    }

    pub async fn run_wrapper(mut self) {
        if let Err(e) = self.run().await {
            log::error!("Client {}: {}", self.id, &e);
        }
        self.shutdown().await;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    let listener = TcpListener::bind(LOCAL_ADDR).await.unwrap();
    log::info!("Version {}\nBound to {}", VERSION, LOCAL_ADDR);

    let mut client_n: usize = 0;
    loop {
        match listener.accept().await {
            Ok((client_sock, addr)) => {
                log::info!("Rec'd connection {} from {:?}", client_n, &addr);
                match TcpStream::connect(SERVER_ADDR).await {
                    Ok(sock) => {
                        log::info!("Client {} connected to server.", client_n);
                        let client = Filter::new(client_n, client_sock, sock);
                        tokio::spawn(async move {
                            client.run_wrapper().await;
                        });
                    }
                    Err(e) => {
                        log::error!(
                            "Client {}: error connnecting with server: {}",
                            client_n, &e
                        );
                    },
                }
                client_n += 1;
            },
            Err(e) => {
                log::error!("Error with incoming client connection: {}", &e);
            }
        }
    }
}