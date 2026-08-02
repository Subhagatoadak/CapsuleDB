use capsule_registry::Registry;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Minimal hand-rolled HTTP/1.1 client for tests only — avoids adding an
/// HTTP client dependency just to exercise our own server. Always sends
/// `Connection: close` so the response can be read to EOF.
fn http(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();

    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();

    let mut parts = raw.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or_default();
    let response_body = parts.next().unwrap_or_default();
    let status: u16 = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let json_body = if response_body.trim().is_empty() { Value::Null } else { serde_json::from_str(response_body).unwrap() };
    (status, json_body)
}

fn start_server() -> SocketAddr {
    let registry = Registry::open_in_memory().unwrap();
    let (addr, _handle) = capsule_api::Server::new(registry).spawn_background().unwrap();
    addr
}

fn draft_capsule_json(logical_id: &str) -> String {
    format!(
        r#"{{
          "schema_version": "kcp/0.1",
          "logical_id": "{logical_id}",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Retain data for 90 days.",
          "epistemic_status": "policy_defined",
          "confidence": {{"basis": "policy_mandate", "ceiling": 0.9}},
          "lifecycle_state": "draft",
          "governance": {{"owner": "alice", "domain": "policy"}}
        }}"#
    )
}

#[test]
fn health_check_returns_ok() {
    let addr = start_server();
    let (status, body) = http(addr, "GET", "/health", None);
    assert_eq!(status, 200);
    assert_eq!(body["status"], "ok");
}

#[test]
fn full_register_transition_publish_get_cycle_over_http() {
    let addr = start_server();
    let logical_id = "kc://policy/retention/http-test";
    let capsule_json = draft_capsule_json(logical_id);

    let (status, body) = http(addr, "POST", "/register?actor=alice", Some(&capsule_json));
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["lifecycle_state"], "draft");

    let encoded_id = "kc%3A%2F%2Fpolicy%2Fretention%2Fhttp-test";

    let (status, _) = http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=in_review&actor=alice"), None);
    assert_eq!(status, 200);

    let (status, _) = http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=approved&actor=alice"), None);
    assert_eq!(status, 200);

    let (status, body) = http(addr, "POST", &format!("/publish?logical_id={encoded_id}&version=1.0.0&actor=alice"), None);
    assert_eq!(status, 200, "{body:?}");

    let (status, body) = http(addr, "GET", &format!("/capsule?logical_id={encoded_id}&version=1.0.0"), None);
    assert_eq!(status, 200);
    assert_eq!(body["lifecycle_state"], "published");
    assert_eq!(body["statement"], "Retain data for 90 days.");

    let (status, body) = http(addr, "GET", &format!("/versions?logical_id={encoded_id}"), None);
    assert_eq!(status, 200);
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[test]
fn get_missing_capsule_returns_404() {
    let addr = start_server();
    let (status, _) = http(addr, "GET", "/capsule?logical_id=kc%3A%2F%2Fa%2Fb%2Fc&version=1.0.0", None);
    assert_eq!(status, 404);
}

#[test]
fn register_invalid_capsule_returns_400() {
    let addr = start_server();
    let bad = r#"{"schema_version": "kcp/0.1", "logical_id": "NOT-VALID", "version": "1.0.0", "capsule_type": "fact", "statement": "x", "epistemic_status": "observed", "confidence": {"basis": "direct_observation", "ceiling": 0.9}, "lifecycle_state": "draft", "governance": {"owner": "a", "domain": "b"}}"#;
    let (status, body) = http(addr, "POST", "/register?actor=alice", Some(bad));
    assert_eq!(status, 400);
    assert!(body["error"].as_str().unwrap().contains("logical_id"));
}

#[test]
fn get_restricted_capsule_requires_matching_consumer_role() {
    let addr = start_server();
    let logical_id = "kc://policy/internal/http-restricted";
    let encoded_id = "kc%3A%2F%2Fpolicy%2Finternal%2Fhttp-restricted";
    let capsule_json = format!(
        r#"{{
          "schema_version": "kcp/0.1",
          "logical_id": "{logical_id}",
          "version": "1.0.0",
          "capsule_type": "rule",
          "statement": "Internal only.",
          "epistemic_status": "policy_defined",
          "confidence": {{"basis": "policy_mandate", "ceiling": 0.9}},
          "lifecycle_state": "draft",
          "governance": {{"owner": "alice", "domain": "policy", "permissions": ["legal-team"]}}
        }}"#
    );

    let (status, _) = http(addr, "POST", "/register?actor=alice", Some(&capsule_json));
    assert_eq!(status, 200);
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=in_review&actor=alice"), None);
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=approved&actor=alice"), None);
    http(addr, "POST", &format!("/publish?logical_id={encoded_id}&version=1.0.0&actor=alice"), None);

    let (status, _) = http(addr, "GET", &format!("/capsule?logical_id={encoded_id}&version=1.0.0"), None);
    assert_eq!(status, 403, "anonymous request must not see a role-restricted capsule");

    let (status, body) = http(addr, "GET", &format!("/capsule?logical_id={encoded_id}&version=1.0.0&consumer_role=legal-team"), None);
    assert_eq!(status, 200, "{body:?}");
}

#[test]
fn resolve_endpoint_returns_bundle() {
    let addr = start_server();
    let logical_id = "kc://policy/retention/resolve-test";
    let capsule_json = draft_capsule_json(logical_id);
    let encoded_id = "kc%3A%2F%2Fpolicy%2Fretention%2Fresolve-test";

    http(addr, "POST", "/register?actor=alice", Some(&capsule_json));
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=in_review&actor=alice"), None);
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=approved&actor=alice"), None);
    http(addr, "POST", &format!("/publish?logical_id={encoded_id}&version=1.0.0&actor=alice"), None);

    let request_body = json!({ "purpose": "check", "domain": "policy", "about": "retain" }).to_string();
    let (status, body) = http(addr, "POST", "/resolve", Some(&request_body));
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body["capsules"].as_array().unwrap().len(), 1);
}
