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

#[test]
fn history_dependencies_and_audit_endpoints_work() {
    let addr = start_server();
    let logical_id = "kc://policy/retention/studio-endpoints";
    let capsule_json = draft_capsule_json(logical_id);
    let encoded_id = "kc%3A%2F%2Fpolicy%2Fretention%2Fstudio-endpoints";

    http(addr, "POST", "/register?actor=alice", Some(&capsule_json));
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=in_review&actor=alice"), None);

    let (status, body) = http(addr, "GET", &format!("/history?logical_id={encoded_id}&version=1.0.0"), None);
    assert_eq!(status, 200);
    assert_eq!(body.as_array().unwrap().len(), 2, "register + one transition");

    let (status, body) = http(addr, "GET", &format!("/dependencies?logical_id={encoded_id}&version=1.0.0"), None);
    assert_eq!(status, 200);
    assert!(body.as_array().unwrap().is_empty());

    let (status, body) = http(addr, "GET", "/audit?limit=5", None);
    assert_eq!(status, 200);
    assert!(!body.as_array().unwrap().is_empty());
}

#[test]
fn evaluate_endpoint_flags_confidence_laundering() {
    let addr = start_server();
    let logical_id = "kc://policy/retention/eval-endpoint";
    let encoded_id = "kc%3A%2F%2Fpolicy%2Fretention%2Feval-endpoint";
    let capsule_json = format!(
        r#"{{
          "schema_version": "kcp/0.1",
          "logical_id": "{logical_id}",
          "version": "1.0.0",
          "capsule_type": "fact",
          "statement": "Retention is estimated at ninety days.",
          "epistemic_status": "estimated",
          "confidence": {{"basis": "statistical_inference", "ceiling": 0.5}},
          "lifecycle_state": "draft",
          "governance": {{"owner": "alice", "domain": "policy"}}
        }}"#
    );

    http(addr, "POST", "/register?actor=alice", Some(&capsule_json));
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=in_review&actor=alice"), None);
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=approved&actor=alice"), None);
    http(addr, "POST", &format!("/publish?logical_id={encoded_id}&version=1.0.0&actor=alice"), None);

    let request_body = json!({ "answer": "Retention is observed at ninety days." }).to_string();
    let (status, body) = http(addr, "POST", "/evaluate", Some(&request_body));
    assert_eq!(status, 200, "{body:?}");
    let findings = body["findings"].as_array().unwrap();
    assert!(findings.iter().any(|f| f["kind"] == "InferenceToFact"), "{findings:?}");
}

/// Regression: GET /capsule must reflect a transition immediately, not a
/// stale copy served from capsule-runtime's cache. Found via manual
/// Capsule Studio testing — register/transition/publish write straight
/// to the registry, so without invalidating the cache afterward, a
/// capsule fetched once (populating the cache) would appear stuck at
/// its first-seen lifecycle state forever.
#[test]
fn get_capsule_reflects_transitions_immediately_not_a_stale_cache_entry() {
    let addr = start_server();
    let logical_id = "kc://policy/retention/cache-staleness";
    let encoded_id = "kc%3A%2F%2Fpolicy%2Fretention%2Fcache-staleness";
    let capsule_json = draft_capsule_json(logical_id);

    http(addr, "POST", "/register?actor=alice", Some(&capsule_json));

    // populate the cache while still draft
    let (_, body) = http(addr, "GET", &format!("/capsule?logical_id={encoded_id}&version=1.0.0"), None);
    assert_eq!(body["lifecycle_state"], "draft");

    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=in_review&actor=alice"), None);
    http(addr, "POST", &format!("/transition?logical_id={encoded_id}&version=1.0.0&to=approved&actor=alice"), None);
    http(addr, "POST", &format!("/publish?logical_id={encoded_id}&version=1.0.0&actor=alice"), None);

    let (_, body) = http(addr, "GET", &format!("/capsule?logical_id={encoded_id}&version=1.0.0"), None);
    assert_eq!(body["lifecycle_state"], "published", "GET /capsule served a stale cached copy");
}

#[test]
fn options_request_gets_cors_preflight_response() {
    let addr = start_server();
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stream.write_all(b"OPTIONS /capsule HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    assert!(raw.starts_with("HTTP/1.1 204"), "{raw}");
    assert!(raw.contains("Access-Control-Allow-Origin: *"));
}
