use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use predicates::prelude::*;

const SEARCH_FIXTURE: &str = include_str!("fixtures/search.json");
const DETAIL_FIXTURE: &str = include_str!("fixtures/detail.json");

fn command() -> Command {
    let mut command = Command::cargo_bin("kleinanzeigen").expect("binary exists");
    command
        .env_remove("KLEINANZEIGEN_BASIC_USER")
        .env_remove("KLEINANZEIGEN_BASIC_PW")
        .env_remove("KLEINANZEIGEN_BASE_URL")
        .env_remove("KLEINANZEIGEN_APP_VERSION");
    command
}

fn serve_once(body: &'static str) -> (String, thread::JoinHandle<()>) {
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

fn server_command(base_url: &str) -> Command {
    let mut command = command();
    command
        .env("KLEINANZEIGEN_BASE_URL", base_url)
        .env("KLEINANZEIGEN_BASIC_USER", "test")
        .env("KLEINANZEIGEN_BASIC_PW", "secret");
    command
}

#[test]
fn doctor_json_is_stable() {
    let output = command()
        .args(["doctor", "--json"])
        .output()
        .expect("command runs");

    assert!(output.status.success());

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["status"], "ok");
    assert_eq!(value["data"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(value["data"]["provider"].is_string());
    assert_eq!(
        value["data"]["provider"],
        "mobile-api (missing credentials)"
    );
}

#[test]
fn json_mode_emits_structured_parse_errors() {
    command()
        .args(["--json", "nope"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("\"schema_version\":1"))
        .stderr(predicate::str::contains("\"code\":\"invalid_input\""))
        .stderr(predicate::str::contains("unrecognized subcommand 'nope'"))
        .stderr(predicate::str::contains("Usage").not());
}

#[test]
fn doctor_human_output_is_readable() {
    command()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("status: ok"))
        .stdout(predicate::str::contains("provider: mobile-api"));
}

#[test]
fn help_lists_the_read_commands() {
    command()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("doctor"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("get"));
}

#[test]
fn unknown_command_exits_with_input_code() {
    command().arg("nope").assert().failure().code(2);
}

#[test]
fn search_json_is_stable() {
    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let output = server_command(&base_url)
        .args([
            "--json",
            "search",
            "synthetic",
            "--cursor",
            "2",
            "--limit",
            "25",
        ])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["total"], 2);
    assert_eq!(value["data"]["page"], 2);
    assert_eq!(value["data"]["size"], 25);
    assert_eq!(value["data"]["next_cursor"], "3");
    assert_eq!(value["data"]["items"][0]["id"], "1234567890");
    assert_eq!(
        value["data"]["items"][0]["title"],
        "Synthetic bike & trailer"
    );
    assert_eq!(
        value["data"]["items"][0]["description"],
        "A synthetic description.\nSecond line."
    );
    assert_eq!(value["data"]["items"][0]["price"]["amount_cents"], 1250);
    assert_eq!(value["data"]["items"][0]["price"]["negotiable"], true);
    assert_eq!(value["data"]["items"][0]["location"]["city"], "Teststadt");
    assert_eq!(
        value["data"]["items"][0]["images"][0],
        "https://img.example.invalid/synthetic-bike-large.jpg"
    );
    assert_eq!(value["data"]["items"][0]["attributes"][0]["value"], "Good");
}

#[test]
fn get_json_is_stable() {
    let (base_url, server) = serve_once(DETAIL_FIXTURE);
    let output = server_command(&base_url)
        .args(["--json", "get", "1234567890"])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["id"], "1234567890");
    assert_eq!(value["data"]["title"], "Synthetic bike & trailer");
    assert_eq!(
        value["data"]["description"],
        "A synthetic description.\nSecond line."
    );
    assert_eq!(value["data"]["price"]["amount_cents"], 1250);
    assert_eq!(value["data"]["price"]["currency"], "EUR");
    assert_eq!(value["data"]["price"]["negotiable"], true);
    assert_eq!(value["data"]["category"]["name"], "Synthetic category");
    assert_eq!(value["data"]["poster_type"], "PRIVATE");
    assert_eq!(value["data"]["status"], "ACTIVE");
}

#[test]
fn get_rejects_a_non_numeric_id() {
    command()
        .args(["--json", "get", "not-a-number"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("\"code\":\"invalid_input\""))
        .stderr(predicate::str::contains("listing id"));
}

#[test]
fn search_without_credentials_fails_closed() {
    command()
        .args(["--json", "search", "synthetic"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("\"code\":\"invalid_input\""))
        .stderr(predicate::str::contains("KLEINANZEIGEN_BASIC_USER"));
}
