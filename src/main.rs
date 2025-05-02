use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

const RESPONSE: &[u8; 7] = b"+PONG\r\n";

fn main() {
    let listener = TcpListener::bind("127.0.0.1:6379").unwrap();
    println!("Listening on 127.0.0.1:6379");

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        handle_connection(stream);
    }
}

fn handle_connection(mut stream: TcpStream) {
    loop {
        let mut buffer = [0; 512];
        let bytes_read = stream
            .read(&mut buffer)
            .expect("Failed to read from stream");
        if bytes_read == 0 {
            // client closed the connection
            break;
        }

        // Optional: inspect input
        // println!("Received: {:?}", &buffer[..bytes_read]);

        stream.write_all(RESPONSE).unwrap();
    }
}
