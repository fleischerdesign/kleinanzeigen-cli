mod support;

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use support::{TempState, serve_once};

const SEARCH_FIXTURE: &str = include_str!("fixtures/search.json");
const DETAIL_FIXTURE: &str = include_str!("fixtures/detail.json");
const PASSWORD: &str = "mcp-test-secret";

const PROFILE_FIXTURE: &str = r#"{
    "id": "42",
    "contactName": "Synthetic Seller",
    "userSince": "2020-01-01",
    "posterType": "PRIVATE",
    "initials": "SS",
    "counters": { "historicalAds": 3, "onlineAds": 2 },
    "replyIndicators": { "replySpeed": "3h" },
    "userBadges": { "badges": [{ "name": "rating", "level": 2, "value": "good" }] },
    "securePayment": true,
    "sellerAccountType": "private"
}"#;

const CATEGORIES_FIXTURE: &str = r#"{
    "{http://www.ebayclassifiedsgroup.com/schema/category/v1}categories": {
        "value": {
            "category": [{
                "id-name": { "value": "Alle Kategorien" },
                "localized-name": { "value": "Alle Kategorien" },
                "category": [{
                    "id": "100",
                    "id-name": { "value": "Synthetic parent" },
                    "localized-name": { "value": "Synthetic parent" },
                    "category": [{
                        "id": "101",
                        "localized-name": { "value": "Synthetic child" }
                    }]
                }]
            }]
        }
    }
}"#;

const LOCATIONS_FIXTURE: &str = r#"{
    "{http://www.ebayclassifiedsgroup.com/schema/location/v1}locations": {
        "value": {
            "location": [{
                "id": "3331",
                "id-name": { "value": "Teststadt" },
                "localized-name": { "value": "Teststadt" },
                "location": [{
                    "id": "3332",
                    "id-name": { "value": "Testdorf" },
                    "localized-name": { "value": "Testdorf (Testland)" }
                }]
            }]
        }
    }
}"#;

const LOCATION_FIXTURE: &str = r#"{
    "{http://www.ebayclassifiedsgroup.com/schema/location/v1}location": {
        "value": {
            "id": "3331",
            "id-name": { "value": "Teststadt" },
            "localized-name": { "value": "Teststadt (Testland)" },
            "parent-id": { "value": "0" },
            "children-count": { "value": "1" },
            "latitude": { "value": "52.5" },
            "longitude": { "value": "13.4" },
            "radius": { "value": "5.0" },
            "location": [{
                "id": "3332",
                "id-name": { "value": "Testdorf" },
                "localized-name": { "value": "Testdorf" }
            }]
        }
    }
}"#;

const METADATA_FIXTURE: &str = r#"{
    "{http://www.ebayclassifiedsgroup.com/schema/ad/v1}ads-search-options": {
        "value": {
            "price": {
                "localized-label": "Price",
                "type": "STRING",
                "deprecated": "false"
            },
            "adType": {
                "localized-label": "Type",
                "type": "ENUM",
                "search-param": "optional",
                "supported-value": [
                    { "value": "OFFERED", "localized-label": "Offer" },
                    { "value": "WANTED", "localized-label": "Want" }
                ],
                "deprecated": "false"
            }
        }
    }
}"#;

const REASONS_FIXTURE: &str = r#"[
    {
        "id": "1",
        "title": "Synthetic reason",
        "submittable": true,
        "allowAnonymous": false,
        "allowGuest": false,
        "textRequired": true,
        "subreasons": [
            { "id": "1.1", "title": "Synthetic subreason", "submittable": true }
        ]
    }
]"#;

const WATCHLIST_FIXTURE: &str =
    r#"{"counters":[{"adId":"1234567890","value":2},{"adId":"1234567891","value":1}]}"#;
const VIEWS_FIXTURE: &str = r#"{"value":7}"#;

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

fn initialize_request(id: u64, version: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": version,
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1" }
        }
    })
}

fn initialized_notification() -> Value {
    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
}

fn run_with_handshake(command: Command, values: &[Value]) -> Session {
    let mut all = vec![
        initialize_request(1000, "2025-06-18"),
        initialized_notification(),
    ];
    all.extend_from_slice(values);
    run_session(command, &requests(&all))
}

fn response_with_id(session: &Session, id: u64) -> &Value {
    session
        .responses
        .iter()
        .find(|response| response["id"] == id)
        .unwrap_or_else(|| panic!("missing response for id {id}"))
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

fn call_with_bodies(tool: &str, arguments: Value, bodies: &[&str]) -> Value {
    let (base_url, server) = support::serve_sequence(
        bodies
            .iter()
            .map(|body| (*body).to_owned())
            .collect::<Vec<_>>(),
    );
    let mut command = mcp_command();
    command
        .env("KLEINANZEIGEN_BASE_URL", &base_url)
        .env("KLEINANZEIGEN_BASIC_USER", "test")
        .env("KLEINANZEIGEN_BASIC_PW", PASSWORD);
    let session = run_with_handshake(command, &[call(2, tool, arguments)]);
    server.join().expect("test server finishes");
    response_with_id(&session, 2)["result"].clone()
}

fn success_data(result: &Value) -> Value {
    assert_eq!(result["isError"], false, "tool failed: {result}");
    let text = result["content"][0]["text"].as_str().expect("text content");
    let envelope: Value = serde_json::from_str(text).expect("envelope is JSON");
    assert_eq!(envelope["schema_version"], 1);
    envelope["data"].clone()
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
    let session = run_with_handshake(
        command,
        &[call(
            1,
            "search",
            json!({ "query": "synthetic", "limit": 2 }),
        )],
    );
    server.join().expect("test server finishes");

    let result = &response_with_id(&session, 1)["result"];
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
fn network_tools_return_successful_envelopes() {
    let data = success_data(&call_with_bodies(
        "get",
        json!({ "id": "1234567890" }),
        &[DETAIL_FIXTURE],
    ));
    assert_eq!(data["id"], "1234567890");

    let data = success_data(&call_with_bodies(
        "similar",
        json!({ "id": "1234567890" }),
        &[SEARCH_FIXTURE],
    ));
    assert_eq!(data["items"].as_array().expect("items").len(), 2);

    let data = success_data(&call_with_bodies(
        "seller_other",
        json!({ "id": "1234567890" }),
        &[SEARCH_FIXTURE],
    ));
    assert_eq!(data["items"].as_array().expect("items").len(), 2);

    let data = success_data(&call_with_bodies("gallery", json!({}), &[SEARCH_FIXTURE]));
    assert_eq!(data["items"].as_array().expect("items").len(), 2);

    let data = success_data(&call_with_bodies(
        "watch_once",
        json!({ "query": "synthetic", "limit": 2 }),
        &[SEARCH_FIXTURE],
    ));
    assert_eq!(data["items"].as_array().expect("items").len(), 2);

    let data = success_data(&call_with_bodies(
        "profile",
        json!({ "seller_id": "42" }),
        &[PROFILE_FIXTURE],
    ));
    assert_eq!(data["id"], "42");
    assert_eq!(data["name"], "Synthetic Seller");

    let data = success_data(&call_with_bodies(
        "stats",
        json!({ "id": "1234567890" }),
        &[WATCHLIST_FIXTURE, VIEWS_FIXTURE],
    ));
    assert_eq!(data["views"], 7);
    assert_eq!(data["watchlist"], 2);

    let data = success_data(&call_with_bodies(
        "stats_batch",
        json!({ "ids": ["1234567890", "1234567891"] }),
        &[WATCHLIST_FIXTURE, VIEWS_FIXTURE, VIEWS_FIXTURE],
    ));
    let stats = data.as_array().expect("batch array");
    assert_eq!(stats.len(), 2);
    assert_eq!(stats[0]["watchlist"], 2);
    assert_eq!(stats[1]["watchlist"], 1);

    let data = success_data(&call_with_bodies(
        "images",
        json!({ "id": "1234567890" }),
        &[DETAIL_FIXTURE],
    ));
    assert_eq!(data["id"], "1234567890");
    assert_eq!(data["variants"].as_array().expect("variants").len(), 2);

    let data = success_data(&call_with_bodies(
        "categories",
        json!({}),
        &[CATEGORIES_FIXTURE],
    ));
    assert_eq!(data.as_array().expect("categories").len(), 2);

    let data = success_data(&call_with_bodies(
        "locations",
        json!({ "query": "Test" }),
        &[LOCATIONS_FIXTURE],
    ));
    assert_eq!(data.as_array().expect("locations").len(), 2);

    let data = success_data(&call_with_bodies(
        "search_metadata",
        json!({ "category": "100" }),
        &[METADATA_FIXTURE],
    ));
    assert_eq!(data.as_array().expect("filters").len(), 2);

    let data = success_data(&call_with_bodies(
        "location",
        json!({ "id": "3331" }),
        &[LOCATION_FIXTURE],
    ));
    assert_eq!(data["id"], "3331");
    assert_eq!(data["children"].as_array().expect("children").len(), 1);

    let data = success_data(&call_with_bodies(
        "flag_reasons",
        json!({}),
        &[REASONS_FIXTURE],
    ));
    assert_eq!(data.as_array().expect("reasons").len(), 1);
    assert_eq!(
        data[0]["subreasons"].as_array().expect("subreasons").len(),
        1
    );
}

#[test]
fn tool_errors_are_structured_results() {
    let session = run_with_handshake(
        mcp_command(),
        &[call(1, "get", json!({ "id": "not-a-number" }))],
    );

    let response = response_with_id(&session, 1);
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(error_code(response), "invalid_input");
}

#[test]
fn unknown_tools_are_protocol_errors() {
    let session = run_with_handshake(mcp_command(), &[call(1, "nope", json!({}))]);

    let response = response_with_id(&session, 1);
    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("message")
            .contains("unknown tool")
    );
}

#[test]
fn tools_require_the_handshake() {
    let session = run_session(
        mcp_command(),
        &requests(&[json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })]),
    );
    assert_eq!(session.responses[0]["error"]["code"], -32002);

    let session = run_session(
        mcp_command(),
        &requests(&[
            initialize_request(1, "2025-06-18"),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        ]),
    );
    assert_eq!(session.responses[1]["error"]["code"], -32002);

    let session = run_session(
        mcp_command(),
        &requests(&[
            initialize_request(1, "2025-06-18"),
            initialize_request(2, "2025-06-18"),
        ]),
    );
    assert_eq!(session.responses[1]["error"]["code"], -32600);
}

#[test]
fn protocol_versions_and_invalid_ids_are_handled() {
    let session = run_session(
        mcp_command(),
        &requests(&[initialize_request(1, "2025-03-26")]),
    );
    assert_eq!(
        session.responses[0]["result"]["protocolVersion"],
        "2025-06-18"
    );

    let session = run_session(
        mcp_command(),
        &requests(&[
            json!({ "jsonrpc": "2.0", "id": null, "method": "ping" }),
            json!([{ "jsonrpc": "2.0", "id": 1, "method": "ping" }]),
        ]),
    );
    assert_eq!(session.responses.len(), 2);
    assert_eq!(session.responses[0]["error"]["code"], -32600);
    assert!(session.responses[0]["id"].is_null());
    assert_eq!(session.responses[1]["error"]["code"], -32600);
    assert!(session.responses[1]["id"].is_null());
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
fn every_argument_tool_reports_missing_input() {
    let cases = [
        ("search", json!({})),
        ("get", json!({})),
        ("similar", json!({})),
        ("seller_other", json!({})),
        ("profile", json!({})),
        ("stats", json!({})),
        ("stats_batch", json!({})),
        ("images", json!({})),
        ("locations", json!({})),
        ("search_metadata", json!({})),
        ("location", json!({})),
        ("saved_show", json!({})),
        ("history", json!({})),
        ("watch_once", json!({})),
    ];
    let values = cases
        .iter()
        .enumerate()
        .map(|(index, (name, arguments))| call(index as u64 + 1, name, arguments.clone()))
        .collect::<Vec<_>>();
    let session = run_with_handshake(mcp_command(), &values);

    assert_eq!(session.responses.len(), cases.len() + 1);
    for (index, (name, _)) in cases.iter().enumerate() {
        let response = response_with_id(&session, index as u64 + 1);
        assert_eq!(response["result"]["isError"], true, "tool {name}");
        assert_eq!(error_code(response), "invalid_input", "tool {name}");
    }
}

#[test]
fn local_state_tools_are_read_only() {
    let state = TempState::new();
    let mut command = mcp_command();
    command.env("KLEINANZEIGEN_STATE_DIR", state.path());
    let session = run_with_handshake(
        command,
        &[
            call(1, "saved_list", json!({})),
            call(2, "saved_show", json!({ "name": "missing" })),
            call(3, "history", json!({ "id": "1234567890" })),
        ],
    );

    let list = response_with_id(&session, 1)["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let list: Value = serde_json::from_str(list).expect("JSON");
    assert!(list["data"].as_array().expect("array").is_empty());

    let missing = response_with_id(&session, 2);
    assert_eq!(missing["result"]["isError"], true);
    assert_eq!(error_code(missing), "not_found");

    let history = response_with_id(&session, 3)["result"]["content"][0]["text"]
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
fn history_tool_reads_recorded_points() {
    let state = TempState::new();
    let (base_url, server) = serve_once(SEARCH_FIXTURE);
    let mut watch = state.command();
    watch
        .env("KLEINANZEIGEN_BASE_URL", &base_url)
        .env("KLEINANZEIGEN_BASIC_USER", "test")
        .env("KLEINANZEIGEN_BASIC_PW", PASSWORD)
        .args([
            "--json",
            "watch",
            "synthetic",
            "--once",
            "--store",
            "--limit",
            "2",
        ]);
    let stored = watch.output().expect("CLI runs");
    server.join().expect("test server finishes");
    assert!(
        stored.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stored.stderr)
    );

    let mut command = mcp_command();
    command.env("KLEINANZEIGEN_STATE_DIR", state.path());
    let session = run_with_handshake(
        command,
        &[call(1, "history", json!({ "id": "1234567890" }))],
    );
    let data = success_data(&response_with_id(&session, 1)["result"]);
    let points = data.as_array().expect("history array");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0]["amount_cents"], 1250);
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
    let session = run_with_handshake(
        command,
        &[
            call(1, "saved_list", json!({})),
            call(2, "saved_show", json!({ "name": "camper" })),
        ],
    );

    let list = response_with_id(&session, 1)["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let list: Value = serde_json::from_str(list).expect("JSON");
    assert_eq!(list["data"].as_array().expect("array").len(), 1);
    assert_eq!(list["data"][0]["name"], "camper");

    let show = response_with_id(&session, 2)["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let show: Value = serde_json::from_str(show).expect("JSON");
    assert_eq!(show["data"]["interval"], "30s");
    assert_eq!(show["data"]["query"]["query"], "fendt bianco");
}
