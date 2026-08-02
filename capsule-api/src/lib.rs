//! `capsule-api`: a minimal HTTP JSON API over `capsule-runtime` — the
//! "remote API runtime" deployment mode from the Phase 5 roadmap. This
//! is the network face of the same runtime `capsule-studio` and any
//! future embedder use: `GET /capsule` and `POST /resolve` go through
//! `capsule_runtime::Runtime`, so policy enforcement, consumer-specific
//! views, local caching, and usage telemetry all apply here too, not
//! just when embedded in-process.
//!
//! Built on `tiny_http` (a small, dependency-light blocking HTTP/1.1
//! server) rather than a full async web framework: this is a single
//! logical service with modest concurrency needs, and pulling in an
//! async runtime for it would be more machinery than the problem calls
//! for. It is single-process, plaintext HTTP, with no auth beyond
//! whatever a reverse proxy in front of it provides — fine for local/dev
//! use and for `sdk-python`/`sdk-typescript` to exercise, not yet a
//! hardened public-facing service (no TLS termination, no rate limiting,
//! no auth — see the security threat model in
//! `docs/security/threat-model.md` for what that implies).
//!
//! Routes use query parameters rather than path segments for capsule
//! identity, since a `logical_id` (`kc://domain/topic/name`) contains
//! slashes that would otherwise collide with path routing:
//!
//! - `GET  /health`
//! - `GET  /capsule?logical_id=...&version=...[&consumer_role=...][&can_read_evidence=true]`
//! - `GET  /versions?logical_id=...`
//! - `GET  /history?logical_id=...&version=...`
//! - `GET  /dependencies?logical_id=...&version=...`
//! - `GET  /dependents?logical_id=...&version=...`
//! - `GET  /evidence?source_id=...`
//! - `GET  /audit[?limit=...]`
//! - `POST /resolve[?consumer_role=...]` — body: [`capsule_resolver::ResolutionRequest`] JSON
//! - `POST /register?actor=...` — body: capsule JSON (must be `draft`)
//! - `POST /transition?logical_id=...&version=...&to=...&actor=...[&reason=...]`
//! - `POST /publish?logical_id=...&version=...&actor=...`
//! - `POST /evaluate` — body: `{"answer": "...", "discretion": "none"|"low"|"medium"|"high"}`
//!
//! CORS is wide open (`Access-Control-Allow-Origin: *`, all methods/headers)
//! so a static page like `capsule-studio` can call this API directly from
//! a browser without a proxy — consistent with this crate having no auth
//! of its own yet either.

use capsule_core::{parse as parse_capsule, validate, LifecycleState, Severity};
use capsule_registry::DependencyRecord;
use capsule_resolver::ResolutionRequest;
use capsule_runtime::{AccessRequest, Runtime, RuntimeError};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tiny_http::{Header, Method, Response, StatusCode};

pub struct Server {
    runtime: Arc<Mutex<Runtime>>,
}

impl Server {
    pub fn new(registry: capsule_registry::Registry) -> Self {
        Self { runtime: Arc::new(Mutex::new(Runtime::new(registry))) }
    }

    /// Binds and serves forever on `addr` (e.g. `"127.0.0.1:8080"`),
    /// blocking the calling thread.
    pub fn run(self, addr: &str) -> std::io::Result<()> {
        let http = tiny_http::Server::http(addr).map_err(|e| std::io::Error::other(e.to_string()))?;
        for request in http.incoming_requests() {
            self.handle(request);
        }
        Ok(())
    }

    /// Binds to an OS-assigned port on localhost and serves on a
    /// background thread, returning the bound address — used by tests
    /// and by embedders that don't want to block.
    pub fn spawn_background(self) -> std::io::Result<(SocketAddr, JoinHandle<()>)> {
        let http = tiny_http::Server::http("127.0.0.1:0").map_err(|e| std::io::Error::other(e.to_string()))?;
        let addr = http.server_addr().to_ip().ok_or_else(|| std::io::Error::other("expected an IP listen address"))?;
        let handle = std::thread::spawn(move || {
            for request in http.incoming_requests() {
                self.handle(request);
            }
        });
        Ok((addr, handle))
    }

    fn handle(&self, mut request: tiny_http::Request) {
        let start = std::time::Instant::now();
        let (path, query) = split_url(request.url());
        let method = request.method().clone();
        let span = tracing::info_span!("request", method = %method, path = %path);
        let _entered = span.enter();

        if method == Method::Options {
            let _ = request.respond(cors_preflight_response());
            return;
        }

        let result = match (&method, path.as_str()) {
            (Method::Get, "/health") => Ok(json_response(200, &json!({ "status": "ok" }))),
            (Method::Get, "/capsule") => self.handle_get_capsule(&query),
            (Method::Get, "/versions") => self.handle_list_versions(&query),
            (Method::Get, "/history") => self.handle_history(&query),
            (Method::Get, "/dependencies") => self.handle_dependencies(&query),
            (Method::Get, "/dependents") => self.handle_dependents(&query),
            (Method::Get, "/evidence") => self.handle_evidence_family(&query),
            (Method::Get, "/audit") => self.handle_audit(&query),
            (Method::Post, "/resolve") => self.handle_resolve(&mut request, &query),
            (Method::Post, "/register") => self.handle_register(&mut request, &query),
            (Method::Post, "/transition") => self.handle_transition(&query),
            (Method::Post, "/publish") => self.handle_publish(&query),
            (Method::Post, "/evaluate") => self.handle_evaluate(&mut request),
            _ => Ok(json_response(404, &json!({ "error": "not found" }))),
        };

        let response = result.unwrap_or_else(|e| json_response(500, &json!({ "error": e })));
        let status = response.status_code().0;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        if status >= 500 {
            tracing::error!(status, elapsed_ms, "request failed");
        } else if status >= 400 {
            tracing::warn!(status, elapsed_ms, "request rejected");
        } else {
            tracing::info!(status, elapsed_ms, "request handled");
        }
        let _ = request.respond(response);
    }

    fn handle_history(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let version = require(query, "version")?;
        let runtime = self.runtime.lock().unwrap();
        let events = runtime.registry().history(logical_id, version).map_err(|e| e.to_string())?;
        let body: Vec<_> = events
            .into_iter()
            .map(|e| json!({ "from_state": e.from_state, "to_state": e.to_state, "actor": e.actor, "reason": e.reason, "occurred_at": e.occurred_at }))
            .collect();
        Ok(json_response(200, &body))
    }

    fn handle_dependencies(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let version = require(query, "version")?;
        let runtime = self.runtime.lock().unwrap();
        let deps = runtime.registry().dependencies_of(logical_id, version).map_err(|e| e.to_string())?;
        Ok(json_response(200, &dependency_records_json(&deps)))
    }

    fn handle_dependents(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let version = require(query, "version")?;
        let runtime = self.runtime.lock().unwrap();
        let deps = runtime.registry().dependents_of(logical_id, version).map_err(|e| e.to_string())?;
        Ok(json_response(200, &dependency_records_json(&deps)))
    }

    fn handle_evidence_family(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let source_id = require(query, "source_id")?;
        let runtime = self.runtime.lock().unwrap();
        let links = runtime.registry().evidence_family(source_id).map_err(|e| e.to_string())?;
        let body: Vec<_> = links.into_iter().map(|l| json!({ "logical_id": l.logical_id, "version": l.version, "authority": l.authority })).collect();
        Ok(json_response(200, &body))
    }

    fn handle_audit(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let limit: u32 = query.get("limit").and_then(|s| s.parse().ok()).unwrap_or(50);
        let runtime = self.runtime.lock().unwrap();
        let events = runtime.registry().audit_log(limit).map_err(|e| e.to_string())?;
        let body: Vec<_> = events
            .into_iter()
            .map(|e| json!({ "event_type": e.event_type, "logical_id": e.logical_id, "version": e.version, "actor": e.actor, "detail": e.detail, "occurred_at": e.occurred_at }))
            .collect();
        Ok(json_response(200, &body))
    }

    fn handle_evaluate(&self, request: &mut tiny_http::Request) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let body = read_body(request)?;
        let payload: EvaluateRequestBody = serde_json::from_str(&body).map_err(|e| format!("invalid request body: {e}"))?;
        let options = capsule_evaluator::EvaluateOptions { discretion: payload.discretion.into(), ..Default::default() };
        let runtime = self.runtime.lock().unwrap();
        let report = capsule_evaluator::evaluate(&payload.answer, runtime.registry(), &capsule_evaluator::NullJudge, &options).map_err(|e| e.to_string())?;
        Ok(json_response(200, &report))
    }

    fn handle_get_capsule(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let version = require(query, "version")?;
        let access = access_from_query(query);
        let runtime = self.runtime.lock().unwrap();
        match runtime.get(logical_id, version, &access) {
            Ok(Some(view)) => Ok(json_response(200, &view.capsule)),
            Ok(None) => Ok(json_response(404, &json!({ "error": "not found" }))),
            Err(e) => Ok(runtime_error_response(&e)),
        }
    }

    fn handle_list_versions(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let runtime = self.runtime.lock().unwrap();
        let versions = runtime.registry().list_versions(logical_id).map_err(|e| e.to_string())?;
        let body: Vec<_> = versions
            .into_iter()
            .map(|r| json!({ "version": r.version, "lifecycle_state": r.lifecycle_state, "content_hash": r.content_hash }))
            .collect();
        Ok(json_response(200, &body))
    }

    fn handle_resolve(&self, request: &mut tiny_http::Request, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let body = read_body(request)?;
        let resolution_request: ResolutionRequest = serde_json::from_str(&body).map_err(|e| format!("invalid request body: {e}"))?;
        let access = access_from_query(query);
        let runtime = self.runtime.lock().unwrap();
        match runtime.resolve(&resolution_request, &access) {
            Ok(bundle) => Ok(json_response(200, &bundle)),
            Err(e) => Ok(runtime_error_response(&e)),
        }
    }

    fn handle_register(&self, request: &mut tiny_http::Request, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let actor = require(query, "actor")?;
        let body = read_body(request)?;
        let capsule = parse_capsule(&body).map_err(|e| format!("invalid capsule JSON: {e}"))?;
        let mut runtime = self.runtime.lock().unwrap();
        match runtime.registry_mut().register(&capsule, actor) {
            Ok(record) => {
                let _ = runtime.poll_updates(); // invalidate any stale cache entry for this logical_id/version
                Ok(json_response(
                    200,
                    &json!({ "logical_id": record.logical_id, "version": record.version, "lifecycle_state": record.lifecycle_state, "content_hash": record.content_hash }),
                ))
            }
            Err(e) => Ok(json_response(400, &json!({ "error": e.to_string() }))),
        }
    }

    fn handle_transition(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let version = require(query, "version")?;
        let to = require(query, "to")?;
        let actor = require(query, "actor")?;
        let reason = query.get("reason").map(|s| s.as_str());
        let to_state: LifecycleState = serde_json::from_value(serde_json::Value::String(to.to_lowercase()))
            .map_err(|_| format!("'{to}' is not a valid lifecycle state"))?;

        let mut runtime = self.runtime.lock().unwrap();
        match runtime.registry_mut().transition(logical_id, version, to_state, actor, reason) {
            Ok(()) => {
                let _ = runtime.poll_updates(); // GET /capsule must reflect the new state immediately, not on the next poll
                Ok(json_response(200, &json!({ "logical_id": logical_id, "version": version, "lifecycle_state": to_state })))
            }
            Err(e) => Ok(json_response(400, &json!({ "error": e.to_string() }))),
        }
    }

    fn handle_publish(&self, query: &HashMap<String, String>) -> Result<Response<std::io::Cursor<Vec<u8>>>, String> {
        let logical_id = require(query, "logical_id")?;
        let version = require(query, "version")?;
        let actor = require(query, "actor")?;
        let mut runtime = self.runtime.lock().unwrap();
        match runtime.registry_mut().publish(logical_id, version, actor) {
            Ok(outcomes) => {
                // Also invalidates the cache for any capsule auto-superseded
                // by this publish, not just this logical_id/version.
                let _ = runtime.poll_updates();
                Ok(json_response(200, &json!({ "outcomes": outcomes.iter().map(|o| json!({
                    "target_logical_id": o.target_logical_id,
                    "target_version": o.target_version,
                    "applied": o.applied,
                    "note": o.note,
                })).collect::<Vec<_>>() })))
            }
            Err(e) => Ok(json_response(400, &json!({ "error": e.to_string() }))),
        }
    }
}

fn access_from_query(query: &HashMap<String, String>) -> AccessRequest {
    AccessRequest {
        consumer_role: query.get("consumer_role").cloned(),
        purpose: query.get("purpose").cloned(),
        can_read_evidence: query.get("can_read_evidence").map(|v| v == "true").unwrap_or(false),
    }
}

fn runtime_error_response(err: &RuntimeError) -> Response<std::io::Cursor<Vec<u8>>> {
    let status = match err {
        RuntimeError::AccessDenied(_) => 403,
        _ => 400,
    };
    json_response(status, &json!({ "error": err.to_string() }))
}

/// Runs `capsule_core::validate` on a raw capsule JSON body and returns
/// the error-level messages, if any — reused by callers that want a
/// pre-check before attempting a mutating call.
pub fn describe_validation_errors(json_body: &str) -> Result<(), Vec<String>> {
    let capsule = match parse_capsule(json_body) {
        Ok(c) => c,
        Err(e) => return Err(vec![e.to_string()]),
    };
    let issues = validate(&capsule);
    let errors: Vec<String> = issues.into_iter().filter(|i| i.severity == Severity::Error).map(|i| i.message).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn require<'a>(query: &'a HashMap<String, String>, key: &str) -> Result<&'a str, String> {
    query.get(key).map(|s| s.as_str()).ok_or_else(|| format!("missing required query parameter '{key}'"))
}

fn read_body(request: &mut tiny_http::Request) -> Result<String, String> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body).map_err(|e| format!("failed to read request body: {e}"))?;
    Ok(body)
}

fn json_response<T: serde::Serialize>(status: u16, body: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{\"error\":\"failed to serialize response\"}".to_vec());
    Response::from_data(bytes)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
        .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
}

fn cors_preflight_response() -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(Vec::new())
        .with_status_code(StatusCode(204))
        .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
        .with_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap())
        .with_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..]).unwrap())
}

fn dependency_records_json(records: &[DependencyRecord]) -> Vec<serde_json::Value> {
    records
        .iter()
        .map(|d| json!({ "logical_id": d.logical_id, "version": d.version, "depends_on_logical_id": d.depends_on_logical_id, "depends_on_version": d.depends_on_version, "relation": d.relation }))
        .collect()
}

#[derive(Deserialize)]
struct EvaluateRequestBody {
    answer: String,
    #[serde(default)]
    discretion: DiscretionParam,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum DiscretionParam {
    None,
    #[default]
    Low,
    Medium,
    High,
}

impl From<DiscretionParam> for capsule_evaluator::DiscretionLevel {
    fn from(value: DiscretionParam) -> Self {
        match value {
            DiscretionParam::None => capsule_evaluator::DiscretionLevel::None,
            DiscretionParam::Low => capsule_evaluator::DiscretionLevel::Low,
            DiscretionParam::Medium => capsule_evaluator::DiscretionLevel::Medium,
            DiscretionParam::High => capsule_evaluator::DiscretionLevel::High,
        }
    }
}

fn split_url(url: &str) -> (String, HashMap<String, String>) {
    match url.split_once('?') {
        Some((path, query)) => (path.to_string(), parse_query(query)),
        None => (url.to_string(), HashMap::new()),
    }
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (percent_decode(k), percent_decode(v)))
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_handles_scheme_and_slashes() {
        assert_eq!(percent_decode("kc%3A%2F%2Fpolicy%2Fretention%2Fx"), "kc://policy/retention/x");
    }

    #[test]
    fn parse_query_splits_pairs() {
        let q = parse_query("logical_id=kc%3A%2F%2Fa%2Fb%2Fc&version=1.0.0");
        assert_eq!(q.get("logical_id").unwrap(), "kc://a/b/c");
        assert_eq!(q.get("version").unwrap(), "1.0.0");
    }

    #[test]
    fn split_url_separates_path_and_query() {
        let (path, query) = split_url("/capsule?logical_id=x&version=1.0.0");
        assert_eq!(path, "/capsule");
        assert_eq!(query.get("version").unwrap(), "1.0.0");

        let (path, query) = split_url("/health");
        assert_eq!(path, "/health");
        assert!(query.is_empty());
    }
}
