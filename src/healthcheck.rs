//! Lets a distroless container probe itself by re-executing the binary, as Go `healthcheck`.
//!
//! Handle it first in `main`, and declare `test: ["CMD", "/api", "healthcheck"]` in compose.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::env::DEFAULT_PORT;

/// How long the probe may take.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// Runs the probe when `argv[1]` is `healthcheck`. It returns `true` when `/health` answered
/// 2xx, so `main` can exit 0, exits the process with status 1 when it did not, and returns
/// `false` for a normal start. The port is `PORT`, default 8080.
pub fn handle(args: impl IntoIterator<Item = String>) -> bool {
    if args.into_iter().nth(1).as_deref() != Some("healthcheck") {
        return false;
    }
    let port = std::env::var("PORT").ok().filter(|p| !p.trim().is_empty()).unwrap_or_else(|| DEFAULT_PORT.to_string());
    if let Err(reason) = probe(port.trim()) {
        eprintln!("healthcheck: {reason}");
        std::process::exit(1);
    }
    true
}

/// Requests `/health` on `127.0.0.1:<port>` and reports whether it answered 2xx. It targets
/// 127.0.0.1, not localhost, which resolves to `::1` first in these containers.
pub fn probe(port: &str) -> Result<(), String> {
    let port: u16 = port.parse().map_err(|_| format!("PORT {port:?} is not a port number"))?;
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, TIMEOUT).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
    stream
        .write_all(format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut head = [0_u8; 12];
    stream.read_exact(&mut head).map_err(|e| e.to_string())?;
    let status = std::str::from_utf8(&head[9..12]).ok().and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
    if (200..300).contains(&status) {
        return Ok(());
    }
    Err(format!("/health answered {status}"))
}
