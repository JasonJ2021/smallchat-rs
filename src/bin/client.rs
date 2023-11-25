use std::time::Duration;

use tokio::{io::AsyncWriteExt, net::TcpSocket, time::sleep};

#[tokio::main]
async fn main() {
    let socket = TcpSocket::new_v4().unwrap();
    let addr = "127.0.0.1:7711".parse().unwrap();
    let mut x = socket.connect(addr).await.unwrap();
    loop {
        x.write(b"Hello, World").await.unwrap();
        sleep(Duration::from_millis(1000)).await;
        // let buffer = [0; 100];
        // x.read(&mut buffer[..]).await.unwrap();
        // println!("Message = {}", String::from_utf8(buffer.to_vec()).unwrap());
    }
}
