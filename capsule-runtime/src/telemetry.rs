//! Usage telemetry: an in-process log of `get`/`resolve` calls. Not
//! persisted — durable usage telemetry belongs in the registry's
//! `audit_events` (Phase 2) or a future Observatory sink (Phase 8); this
//! is the runtime-local counter/log a process can inspect or flush.

use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct UsageEvent {
    pub operation: &'static str,
    pub logical_id: String,
    pub version: Option<String>,
    pub consumer_role: Option<String>,
}

#[derive(Default)]
pub struct Telemetry {
    events: Mutex<Vec<UsageEvent>>,
}

impl Telemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, event: UsageEvent) {
        self.events.lock().unwrap().push(event);
    }

    pub fn events(&self) -> Vec<UsageEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_lists_events() {
        let telemetry = Telemetry::new();
        telemetry.record(UsageEvent { operation: "get", logical_id: "kc://a/b/c".to_string(), version: Some("1.0.0".to_string()), consumer_role: None });
        assert_eq!(telemetry.count(), 1);
        assert_eq!(telemetry.events()[0].operation, "get");
    }
}
