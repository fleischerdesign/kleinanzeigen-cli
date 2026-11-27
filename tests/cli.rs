mod support;

use assert_cmd::Command;
use predicates::prelude::*;

use support::{TempState, cli_command as command, serve_once};

const SEARCH_FIXTURE: &str = include_str!("fixtures/search.json");
const DETAIL_FIXTURE: &str = include_str!("fixtures/detail.json");

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
    assert_eq!(value["data"]["pages_fetched"], 1);
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
fn watch_once_emits_snapshot_ndjson() {
    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let output = server_command(&base_url)
        .args(["--json", "watch", "synthetic", "--once", "--limit", "2"])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let events = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("event is JSON"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    for event in events {
        assert_eq!(event["schema_version"], 1);
        assert_eq!(event["event"], "snapshot");
        assert!(event["data"]["id"].is_string());
    }
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

#[test]
fn saved_search_lifecycle_is_local() {
    let state = TempState::new();
    let add = state
        .command()
        .args([
            "--json",
            "saved",
            "add",
            "bikes",
            "synthetic bike",
            "--max-price",
            "500",
            "--limit",
            "5",
        ])
        .output()
        .expect("command runs");
    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&add.stdout).expect("stdout is JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["name"], "bikes");
    assert_eq!(value["data"]["interval"], "15m");
    assert_eq!(value["data"]["query"]["max_price_cents"], 50000);
    assert_eq!(value["data"]["query"]["limit"], 5);
    assert!(value["data"]["last_run"].is_null());

    let list = state
        .command()
        .args(["--json", "saved", "list"])
        .output()
        .expect("command runs");
    assert!(list.status.success());
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).expect("JSON");
    assert_eq!(value["data"].as_array().expect("array").len(), 1);
    assert_eq!(value["data"][0]["name"], "bikes");

    let replace = state
        .command()
        .args(["--json", "saved", "add", "bikes", "synthetic lamp"])
        .output()
        .expect("command runs");
    assert!(replace.status.success());
    let list = state
        .command()
        .args(["--json", "saved", "list"])
        .output()
        .expect("command runs");
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).expect("JSON");
    assert_eq!(value["data"].as_array().expect("array").len(), 1);
    assert_eq!(value["data"][0]["query"]["query"], "synthetic lamp");

    let remove = state
        .command()
        .args(["--json", "saved", "remove", "bikes"])
        .output()
        .expect("command runs");
    assert!(remove.status.success());
    let value: serde_json::Value = serde_json::from_slice(&remove.stdout).expect("JSON");
    assert_eq!(value["data"]["removed"], true);

    let missing = state
        .command()
        .args(["--json", "saved", "show", "bikes"])
        .output()
        .expect("command runs");
    assert_eq!(missing.status.code(), Some(3));
    let value: serde_json::Value = serde_json::from_slice(&missing.stderr).expect("stderr is JSON");
    assert_eq!(value["error"]["code"], "not_found");
}

#[test]
fn read_only_state_commands_do_not_create_a_database() {
    let state = TempState::new();

    let history = state
        .command()
        .args(["--json", "history", "1234567890"])
        .output()
        .expect("command runs");
    assert!(history.status.success());
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).expect("JSON");
    assert!(value["data"].as_array().expect("array").is_empty());

    let list = state
        .command()
        .args(["--json", "saved", "list"])
        .output()
        .expect("command runs");
    assert!(list.status.success());
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).expect("JSON");
    assert!(value["data"].as_array().expect("array").is_empty());

    assert!(
        !state.path().join("state.db").exists(),
        "read-only state commands must not create the database"
    );
}

#[test]
fn watch_store_records_history_and_resumes() {
    let state = TempState::new();
    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let output = server_command(&base_url)
        .env("KLEINANZEIGEN_STATE_DIR", state.path())
        .args([
            "--json",
            "watch",
            "synthetic",
            "--once",
            "--store",
            "--limit",
            "2",
        ])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events = String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("event is JSON"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| event["event"] == "snapshot"));

    let history = state
        .command()
        .args(["--json", "history", "1234567890"])
        .output()
        .expect("command runs");
    assert!(
        history.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&history.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).expect("stdout is JSON");
    assert_eq!(value["data"].as_array().expect("array").len(), 1);
    assert_eq!(value["data"][0]["amount_cents"], 1250);
    assert_eq!(value["data"][0]["currency"], "EUR");

    let export = state
        .command()
        .args(["export", "--listing", "1234567890"])
        .output()
        .expect("command runs");
    assert!(export.status.success());
    let rows = String::from_utf8(export.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("row is JSON"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["listing_id"], "1234567890");
    assert_eq!(rows[0]["amount_cents"], 1250);

    let csv = state
        .command()
        .args(["export", "--format", "csv"])
        .output()
        .expect("command runs");
    assert!(csv.status.success());
    let csv = String::from_utf8(csv.stdout).expect("stdout is UTF-8");
    assert!(csv.starts_with("listing_id,seen_at,amount_cents,currency,price_type\n"));
    assert!(csv.contains("1234567890,"), "csv was: {csv}");
    assert!(csv.contains(",1250,EUR,PLEASE_CONTACT\n"), "csv was: {csv}");

    let bounded = state
        .command()
        .args(["export", "--limit", "1"])
        .output()
        .expect("command runs");
    assert!(bounded.status.success());
    assert_eq!(
        String::from_utf8(bounded.stdout)
            .expect("stdout is UTF-8")
            .lines()
            .count(),
        1
    );

    let listings = state
        .command()
        .args(["export", "--kind", "listings", "--listing", "1234567890"])
        .output()
        .expect("command runs");
    assert!(
        listings.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&listings.stderr)
    );
    let rows = String::from_utf8(listings.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("row is JSON"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "1234567890");
    assert_eq!(rows[0]["title"], "Synthetic bike & trailer");
    assert_eq!(rows[0]["last_price_cents"], 1250);
    assert_eq!(rows[0]["city"], "Teststadt");

    let changed = SEARCH_FIXTURE.replace("\"value\": 12.5", "\"value\": 15.0");
    assert_ne!(changed, SEARCH_FIXTURE, "fixture price must change");
    let (base_url, server) = serve_once(&changed);
    let refreshed = server_command(&base_url)
        .env("KLEINANZEIGEN_STATE_DIR", state.path())
        .args([
            "--json",
            "watch",
            "synthetic",
            "--once",
            "--fresh",
            "--store",
            "--limit",
            "2",
        ])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");
    assert!(
        refreshed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    assert_eq!(
        String::from_utf8(refreshed.stdout)
            .expect("stdout is UTF-8")
            .lines()
            .count(),
        2
    );

    let history = state
        .command()
        .args(["--json", "history", "1234567890"])
        .output()
        .expect("command runs");
    let value: serde_json::Value = serde_json::from_slice(&history.stdout).expect("stdout is JSON");
    let points = value["data"].as_array().expect("array");
    assert_eq!(points.len(), 2);
    assert_eq!(points[1]["amount_cents"], 1500);

    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let resumed = server_command(&base_url)
        .env("KLEINANZEIGEN_STATE_DIR", state.path())
        .args([
            "--json",
            "watch",
            "synthetic",
            "--once",
            "--resume",
            "--limit",
            "2",
        ])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");
    assert!(
        resumed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert!(
        resumed.stdout.is_empty(),
        "resume must not repeat seen listings: {}",
        String::from_utf8_lossy(&resumed.stdout)
    );
}

#[test]
fn watch_uses_a_saved_search() {
    let state = TempState::new();
    let add = state
        .command()
        .args([
            "--json",
            "saved",
            "add",
            "lamp",
            "synthetic",
            "--interval",
            "30s",
        ])
        .output()
        .expect("command runs");
    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let output = server_command(&base_url)
        .env("KLEINANZEIGEN_STATE_DIR", state.path())
        .args(["--json", "watch", "--saved", "lamp", "--once"])
        .output()
        .expect("command runs");
    server.join().expect("server finishes");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let show = state
        .command()
        .args(["--json", "saved", "show", "lamp"])
        .output()
        .expect("command runs");
    assert!(show.status.success());
    let value: serde_json::Value = serde_json::from_slice(&show.stdout).expect("JSON");
    assert!(value["data"]["last_run"].is_number());
}

#[test]
fn saved_filters_cannot_be_combined_with_a_saved_name() {
    let state = TempState::new();
    state
        .command()
        .args(["--json", "watch", "--saved", "lamp", "--limit", "5"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("cannot be combined with --saved"));
}
