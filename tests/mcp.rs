mod support;

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use support::{TempState, serve_once};

const SEARCH_FIXTURE: &str = include_str!("fixtures/search.json");
const PASSWORD: &str = "mcp-test-secret";

struct Session {
    responses: Vec<Value>,
    stdout: String,
    stderr: String,
}

fn mcp_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kleinanzeigen-mcp"));
    command
        .env_remove("KLEINANZEIGEN_BASIC_USER")
        .env_remove("KLEINANZEIGEN_BASIC_PW")
        .env_remove("KLEINANZEIGEN_BASE_URL")
        .env_remove("KLEINANZEIGEN_APP_VERSION")
        .env_remove("KLEINANZEIGEN_STATE_DIR");
    command
}

fn run_session(mut command: Command, lines: &[String]) -> Session {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("MCP server starts");
    {
        let stdin = child.stdin.as_mut().expect("stdin is piped");
        for line in lines {
            writeln!(stdin, "{line}").expect("write request");
        }
    }
    let output = child.wait_with_output().expect("MCP server output");
    assert!(
        output.status.success(),
        "MCP server failed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let responses = stdout
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("response is JSON"))
        .collect();
    Session {
        responses,
        stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn requests(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .map(|value| serde_json::to_string(value).expect("request JSON"))
        .collect()
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    })
}

fn error_code(response: &Value) -> String {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("error content is text");
    let value: Value = serde_json::from_str(text).expect("error content is JSON");
    value["error"]["code"]
        .as_str()
        .expect("error code")
        .to_owned()
}

#[test]
fn initialize_and_tools_list_are_stable() {
    let lines = requests(&[
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    ]);
    let session = run_session(mcp_command(), &lines);

    assert_eq!(session.responses.len(), 2);
    let initialize = &session.responses[0]["result"];
    assert_eq!(initialize["protocolVersion"], "2025-06-18");
    assert_eq!(initialize["serverInfo"]["name"], "kleinanzeigen");
    assert_eq!(
        initialize["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(initialize["capabilities"]["tools"].is_object());

    let tools = session.responses[1]["result"]["tools"]
        .as_array()
        .expect("tools array");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 18);
    for expected in [
        "search",
        "get",
        "similar",
        "seller_other",
        "gallery",
        "profile",
        "stats",
        "stats_batch",
        "images",
        "categories",
        "locations",
        "search_metadata",
        "location",
        "flag_reasons",
        "saved_list",
        "saved_show",
        "history",
        "watch_once",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(
            !tool["description"]
                .as_str()
                .expect("description")
                .is_empty()
        );
    }
}

#[test]
fn search_tool_returns_the_cli_envelope_without_secrets() {
    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let mut command = mcp_command();
    command
        .env("KLEINANZEIGEN_BASE_URL", &base_url)
        .env("KLEINANZEIGEN_BASIC_USER", "test")
        .env("KLEINANZEIGEN_BASIC_PW", PASSWORD);
    let lines = requests(&[call(
        1,
        "search",
        json!({ "query": "synthetic", "limit": 2 }),
    )]);
    let session = run_session(command, &lines);
    server.join().expect("test server finishes");

    let result = &session.responses[0]["result"];
    assert_eq!(result["isError"], false);
    let text = result["content"][0]["text"].as_str().expect("text content");
    let value: Value = serde_json::from_str(text).expect("envelope is JSON");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["data"]["total"], 2);
    assert_eq!(value["data"]["items"][0]["id"], "1234567890");
    assert_eq!(
        value["data"]["items"][0]["title"],
        "Synthetic bike & trailer"
    );
    assert!(!session.stdout.contains(PASSWORD));
    assert!(!session.stderr.contains(PASSWORD));
}

#[test]
fn tool_errors_are_structured_results() {
    let lines = requests(&[call(1, "get", json!({ "id": "not-a-number" }))]);
    let session = run_session(mcp_command(), &lines);

    let result = &session.responses[0]["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(error_code(&session.responses[0]), "invalid_input");
}

#[test]
fn unknown_tools_are_protocol_errors() {
    let lines = requests(&[call(1, "nope", json!({}))]);
    let session = run_session(mcp_command(), &lines);

    let response = &session.responses[0];
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("message")
            .contains("unknown tool")
    );
}

#[test]
fn notifications_get_no_response_and_ping_works() {
    let lines = requests(&[
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }),
    ]);
    let session = run_session(mcp_command(), &lines);

    assert_eq!(session.responses.len(), 1);
    assert_eq!(session.responses[0]["id"], 7);
    assert_eq!(session.responses[0]["result"], json!({}));
}

#[test]
fn malformed_json_is_a_parse_error() {
    let lines = vec![
        "{not json".to_owned(),
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string(),
    ];
    let session = run_session(mcp_command(), &lines);

    assert_eq!(session.responses.len(), 2);
    assert_eq!(session.responses[0]["error"]["code"], -32700);
    assert!(session.responses[0]["id"].is_null());
    assert_eq!(session.responses[1]["result"], json!({}));
}

#[test]
fn local_state_tools_are_read_only() {
    let state = TempState::new();
    let mut command = mcp_command();
    command.env("KLEINANZEIGEN_STATE_DIR", state.path());
    let lines = requests(&[
        call(1, "saved_list", json!({})),
        call(2, "saved_show", json!({ "name": "missing" })),
        call(3, "history", json!({ "id": "1234567890" })),
    ]);
    let session = run_session(command, &lines);

    let list = session.responses[0]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let list: Value = serde_json::from_str(list).expect("JSON");
    assert!(list["data"].as_array().expect("array").is_empty());

    assert_eq!(session.responses[1]["result"]["isError"], true);
    assert_eq!(error_code(&session.responses[1]), "not_found");

    let history = session.responses[2]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let history: Value = serde_json::from_str(history).expect("JSON");
    assert!(history["data"].as_array().expect("array").is_empty());

    assert!(
        !state.path().join("state.db").exists(),
        "MCP reads must not create the state database"
    );
}

#[test]
fn saved_searches_created_by_the_cli_are_visible() {
    let state = TempState::new();
    let add = state
        .command()
        .args([
            "--json",
            "saved",
            "add",
            "camper",
            "fendt bianco",
            "--interval",
            "30s",
        ])
        .output()
        .expect("CLI runs");
    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let mut command = mcp_command();
    command.env("KLEINANZEIGEN_STATE_DIR", state.path());
    let lines = requests(&[
        call(1, "saved_list", json!({})),
        call(2, "saved_show", json!({ "name": "camper" })),
    ]);
    let session = run_session(command, &lines);

    let list = session.responses[0]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let list: Value = serde_json::from_str(list).expect("JSON");
    assert_eq!(list["data"].as_array().expect("array").len(), 1);
    assert_eq!(list["data"][0]["name"], "camper");

    let show = session.responses[1]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let show: Value = serde_json::from_str(show).expect("JSON");
    assert_eq!(show["data"]["interval"], "30s");
    assert_eq!(show["data"]["query"]["query"], "fendt bianco");
}
