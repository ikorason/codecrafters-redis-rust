use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Instant;

type Storage = Rc<RefCell<HashMap<String, (String, Option<Instant>)>>>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:6379").await?;
    println!("Server listening on 127.0.0.1:6379");

    let storage = Rc::new(RefCell::new(HashMap::new()));

    let local = tokio::task::LocalSet::new();

    local
        .run_until(async move {
            loop {
                let (stream, _addr) = listener.accept().await.unwrap();
                let storage_clone = Rc::clone(&storage);
                tokio::task::spawn_local(async move {
                    if let Err(e) = handle_connection(stream, storage_clone).await {
                        eprintln!("Error handling connection: {}", e);
                    }
                });
            }
        })
        .await;

    Ok(())
}

async fn handle_connection(mut stream: TcpStream, storage: Storage) -> std::io::Result<()> {
    let mut buffer = [0; 512];

    loop {
        // Read data from the stream
        let n = stream.read(&mut buffer).await?;

        // Connection closed
        if n == 0 {
            break;
        }

        // Parse the buffer (only the bytes that were read)
        if let Some(parts) = parse_resp_array(&buffer[..n]) {
            if parts.is_empty() {
                continue;
            }

            let response = match parts[0].to_uppercase().as_str() {
                "PING" => "+PONG\r\n".to_string(),
                "ECHO" if parts.len() >= 2 => {
                    format!("${}\r\n{}\r\n", parts[1].len(), parts[1])
                }
                "SET" if parts.len() >= 3 => {
                    let key = parts[1].clone();
                    let value = parts[2].clone();

                    // Check for PX option
                    let expiry = if parts.len() >= 5 && parts[3].to_uppercase() == "PX" {
                        if let Ok(ms) = parts[4].parse::<u64>() {
                            Some(Instant::now() + tokio::time::Duration::from_millis(ms))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    storage.borrow_mut().insert(key, (value, expiry));
                    "+OK\r\n".to_string()
                }
                "GET" if parts.len() >= 2 => {
                    let key = &parts[1];
                    let mut store = storage.borrow_mut();

                    match store.get(key) {
                        Some((value, expiry)) => {
                            // Check if key has expired
                            if let Some(exp_time) = expiry {
                                if Instant::now() > *exp_time {
                                    // Key expired, remove it
                                    store.remove(key);
                                    "$-1\r\n".to_string()
                                } else {
                                    // Key still valid
                                    format!("${}\r\n{}\r\n", value.len(), value)
                                }
                            } else {
                                // No expiry
                                format!("${}\r\n{}\r\n", value.len(), value)
                            }
                        }
                        None => "$-1\r\n".to_string(),
                    }
                }
                _ => {
                    eprintln!("Unknown command or insufficient arguments");
                    continue;
                }
            };

            stream.write_all(response.as_bytes()).await?;
        }
    }

    Ok(())
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
