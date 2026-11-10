use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use serde_json::Value;

pub struct Captured {
    pub request_line: String,
    pub authorization: String,
    pub body: Value,
}

pub fn fake_journal(statuses: Vec<u16>) -> (String, mpsc::Receiver<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for (stream, status) in listener.incoming().zip(statuses) {
            let mut stream = stream.expect("stream");
            let captured = read_request(&mut stream);
            let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = sender.send(captured);
        }
    });
    (url, receiver)
}

fn read_request(stream: &mut std::net::TcpStream) -> Captured {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("line");
    let (mut length, mut authorization) = (0, String::new());
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("header");
        let Some((name, value)) = line.trim_end().split_once(": ") else { break };
        match name.to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().expect("length"),
            "authorization" => authorization = value.to_owned(),
            _ => {}
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("body");
    Captured {
        request_line: request_line.trim_end().to_owned(),
        authorization,
        body: serde_json::from_slice(&body).expect("json"),
    }
}
