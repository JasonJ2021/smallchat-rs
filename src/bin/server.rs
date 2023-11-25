use std::str::from_utf8;
use std::sync::Arc;

use crossbeam::epoch::Shared;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::{
    io::{self},
    net::{TcpListener, TcpSocket, TcpStream},
    select,
    sync::{mpsc, Mutex},
};
const MAX_CLIENTS: usize = 1000;
const SERVER_PORT: u32 = 7711;
const SERVER_IP: &'static str = "127.0.0.1";

struct Client {
    nick_name: String,
    sender: UnboundedSender<String>,
}

struct SharedState {
    numclients: usize,
    maxclients: i32,
    clients: Vec<Option<Client>>,
}

impl Client {
    fn new(nick_name: String, sender: UnboundedSender<String>) -> Self {
        Self { nick_name, sender }
    }

    async fn send_to_client(stream: &TcpStream, buf: &[u8]) {
        if let Ok(()) = stream.writable().await {
            match stream.try_write(buf) {
                Ok(_) => {}
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // TODO: find out what is WouldBlock
                }
                Err(e) => {
                    eprintln!("Error:{};while sending message to client", e);
                }
            }
        }
    }

    async fn process(
        client_id: usize,
        stream: TcpStream,
        mut receiver: UnboundedReceiver<String>,
        shared_state: Arc<Mutex<SharedState>>,
    ) {
        loop {
            // 1. receive message from stream, send it to all clients
            // 2. receive message from receiver, send it to stream
            select! {
                    _ = stream.readable() => {
                        let mut data = vec![0; 1024];
                        match stream.try_read(&mut data) {
                            Ok(0) => {
                                break;
                            },
                            Ok(n) => {
                                println!("read {} bytes", n);
                                // println!("message {}", message);
                                /* If the user message starts with "/", we
                                * process it as a client command. So far
                                * only the /nick <newnick> command is implemented. */
                                if data[0] == b'/' {
                                    let message = String::from_utf8(data).expect("Convert utf8-vector to String failed");
                                    let args = message.contains(' ');
                                    let command = message.split(' ').collect::<Vec<_>>();
                                    match (command[0], args) {
                                        ("/nick", true) => {
                                            if command.len() != 2 {
                                                // TODO: print some message to client
                                                Client::send_to_client(&stream, b"Invalid command").await;
                                            } else {
                                                let mut shared_state = shared_state.lock().await;
                                                let new_nick_name = command[1].to_string();
                                                shared_state.clients[client_id].as_mut().unwrap().nick_name = command[1].to_string();
                                                   println!("Now user{}'s nickName has changed to {new_nick_name}", client_id);
                                            }
                                        },
                                        _ => {
                                            // TODO: send unsupported command to client
                                            Client::send_to_client(&stream, b"Unsupported command").await;
                                        }
                                    }
                                } else {
                                    /* Create a message to send everybody (and show
                                    * on the server console) in the form:
                                    *   nick> some message. */
                                    let client_name: String;
                                    let shared_state = shared_state.lock().await;
                                    client_name = shared_state.clients[client_id].as_ref().unwrap().nick_name.clone();
                                    let msg = format!("{}> {}",client_name, from_utf8(&data).expect("Convert message data[u8] to str failed"));
                                    println!("{msg}");
                                    shared_state.sendMsgToAllClientsBut(client_id, msg.as_str()).expect("TODO");
                                }
                            },
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                continue;
                            },
                            Err(e) => {
                                // TODO: connection reset
                                eprintln!("Error: {} ;while processing user:{} messages", e, client_id);
                                return;
                            },
                        }
                    },
                    Some(msg) = receiver.recv() => {
                        println!("Get message {msg}");
                    }

            }
        }
    }
}

impl SharedState {
    fn new() -> io::Result<Self> {
        let addr = format!("{}:{}", SERVER_IP, SERVER_PORT).parse().unwrap();
        let socket = TcpSocket::new_v4()?;
        socket.bind(addr)?;
        Ok(SharedState {
            numclients: 0,
            maxclients: -1,
            clients: std::iter::repeat_with(|| None)
                .take(MAX_CLIENTS)
                .collect::<Vec<_>>(),
        })
    }
    fn create_listener() -> io::Result<TcpListener> {
        let addr = format!("{}:{}", SERVER_IP, SERVER_PORT).parse().unwrap();
        let socket = TcpSocket::new_v4()?;
        socket.bind(addr)?;
        let listener = socket.listen(1024)?;
        Ok(listener)
    }

    // TODO: SendError在这个函数内部处理
    fn sendMsgToAllClientsBut(&self, excluded: usize, msg: &str) -> Result<(), SendError<String>> {
        for i in 0..self.maxclients as usize {
            if self.clients[i].is_none() || i == excluded {
                continue;
            }
            self.clients[i]
                .as_ref()
                .unwrap()
                .sender
                .send(msg.to_string())?;
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("Hello, World");
    let server = Arc::new(Mutex::new(
        SharedState::new().expect("Create chat server failed"),
    ));
    let listner_tcp_listener =
        SharedState::create_listener().expect("Create chat server TcpListener failed");
    loop {
        let (stream, _) = listner_tcp_listener.accept().await?;
        println!("Accept a connection request");
        {
            let server_state = server.clone();
            let (s, r) = mpsc::unbounded_channel::<String>();
            let mut server = server.lock().await;
            let mut client_id: i32 = -1;
            if server.maxclients == -1 {
                client_id = 0;
                server.maxclients = 0;
            } else {
                // Find a free loc for new client
                // TODO: Clients array overflow
                for (idx, client) in server.clients.iter().enumerate() {
                    if idx == server.maxclients as usize {
                        // Will not overflow
                        client_id = server.maxclients + 1;
                        server.maxclients = client_id;
                        break;
                    }
                    if client.is_none() {
                        client_id = idx as i32;
                    }
                }
            }
            assert!(client_id >= 0);
            server.numclients += 1;
            let client = Client::new(format!("user:{}", client_id), s);
            server.clients[client_id as usize] = Some(client);
            tokio::spawn(async move {
                println!("Trying accepting user:{} messages", client_id);
                Client::process(client_id as usize, stream, r, server_state).await;
            });
        }
    }
}
