use std::{
    collections::HashMap,
    io::{Read, Write},
};

use mio::{
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Token,
};

const SERVER: Token = Token(0);

fn main() {
    // Create a poll instance
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(128);

    // Setup the TCP listener
    let addr = "127.0.0.1:6379".parse().unwrap();
    let mut listener = TcpListener::bind(addr).unwrap();

    // Register the listener with the poll instance
    poll.registry()
        .register(&mut listener, SERVER, Interest::READABLE)
        .unwrap();

    println!("Redis clone listening on 127.0.0.1:6379");

    // Map to store client connections
    let mut connections: HashMap<Token, TcpStream> = HashMap::new();
    let mut unique_token = Token(1);

    // Event loop
    loop {
        // Poll for events
        poll.poll(&mut events, None).unwrap();

        for event in events.iter() {
            match event.token() {
                SERVER => {
                    // Accept new connections
                    loop {
                        match listener.accept() {
                            Ok((mut connection, address)) => {
                                println!("Accepted connection from: {}", address);

                                let token = next_token(&mut unique_token);

                                // Register the new connection
                                poll.registry()
                                    .register(&mut connection, token, Interest::READABLE)
                                    .unwrap();

                                connections.insert(token, connection);
                            }
                            Err(ref err) if would_block(err) => break,
                            Err(err) => {
                                eprintln!("Error accepting connection: {}", err);
                                break;
                            }
                        }
                    }
                }
                token => {
                    // Handle client connection
                    let done = if let Some(connection) = connections.get_mut(&token) {
                        handle_connection_event(connection)
                    } else {
                        false
                    };

                    if done {
                        // Client disconnected, remove from map
                        if let Some(mut connection) = connections.remove(&token) {
                            poll.registry().deregister(&mut connection).unwrap();
                            println!("Client disconnected: {:?}", token);
                        }
                    }
                }
            }
        }
    }
}

fn next_token(current: &mut Token) -> Token {
    let next = current.0;
    current.0 += 1;
    Token(next)
}

fn would_block(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::WouldBlock
}

fn handle_connection_event(connection: &mut TcpStream) -> bool {
    let mut buffer = [0u8; 512];

    // Try to read from the connection
    loop {
        match connection.read(&mut buffer) {
            Ok(0) => {
                // Connection closed by client
                return true;
            }
            Ok(n) => {
                if let Some(parts) = parse_resp_array(&buffer[..n]) {
                    eprintln!("Parsed: {:?}", parts);
                    if parts.len() >= 2 && parts[0].to_uppercase() == "ECHO" {
                        let message = &parts[1];
                        let response = format!("${}\r\n{}\r\n", message.len(), message);
                        eprintln!("Sending: {:?}", response);
                        eprintln!("Bytes: {:?}", response.as_bytes());
                        if let Err(e) = connection.write_all(response.as_bytes()) {
                            eprintln!("Failed to send response: {}", e);
                            return true;
                        }
                        eprintln!("Write succeeded!");
                        break; // ADD THIS - exit the loop after responding
                    }
                } else {
                    eprintln!("Parse failed!");
                }
            }
            Err(ref err) if would_block(err) => {
                // No more data available right now
                break;
            }
            Err(err) => {
                eprintln!("Error reading from connection: {}", err);
                return true;
            }
        }
    }

    false
}

fn parse_resp_array(buffer: &[u8]) -> Option<Vec<String>> {
    let data = String::from_utf8_lossy(buffer);
    let mut lines = data.split("\r\n");

    // First line should be *N (array length)
    let array_line = lines.next()?;
    if !array_line.starts_with('*') {
        return None;
    }

    let count: usize = array_line[1..].parse().ok()?;
    let mut result = Vec::new();

    for _ in 0..count {
        // Read bulk string: $length\r\n
        let length_line = lines.next()?;
        if !length_line.starts_with('$') {
            return None;
        }

        // Read the actual content
        let content = lines.next()?;
        result.push(content.to_string());
    }

    Some(result)
}
