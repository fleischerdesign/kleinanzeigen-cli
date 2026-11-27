//! Shared helpers for the integration tests.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;

/// Builds the CLI command with a clean provider environment.
pub fn cli_command() -> Command {
    let mut command = Command::cargo_bin("kleinanzeigen").expect("binary exists");
    command
        .env_remove("KLEINANZEIGEN_BASIC_USER")
        .env_remove("KLEINANZEIGEN_BASIC_PW")
        .env_remove("KLEINANZEIGEN_BASE_URL")
        .env_remove("KLEINANZEIGEN_APP_VERSION")
        .env_remove("KLEINANZEIGEN_STATE_DIR");
    command
}

/// A temporary state directory that is removed on drop.
pub struct TempState(PathBuf);

impl TempState {
    pub fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("kleinanzeigen-cli-state-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("temporary state directory");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn command(&self) -> Command {
        let mut command = cli_command();
        command.env("KLEINANZEIGEN_STATE_DIR", self.path());
        command
    }
}

impl Drop for TempState {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Serves one synthetic body over HTTP on a loopback port.
pub fn serve_once(body: &str) -> (String, thread::JoinHandle<()>) {
    let body = body.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    listener
        .set_nonblocking(true)
        .expect("non-blocking test server");
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept request: {error}"),
            }
        };
        stream
            .set_nonblocking(false)
            .expect("blocking client stream");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .expect("write timeout");

        let mut stream = stream;
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("read request line");
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).expect("read header");
            if header == "\r\n" || header.is_empty() {
                break;
            }
        }

        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
    });

    (format!("http://{address}"), handle)
}
