# sdk-python

A thin, stdlib-only HTTP client for `capsule-api` (Phase 5).

```python
from capsuledb import CapsuleClient

client = CapsuleClient("http://127.0.0.1:8089")
client.health()
record = client.register(capsule_json, actor="alice")
client.transition(record["logical_id"], record["version"], "in_review", actor="alice")
client.transition(record["logical_id"], record["version"], "approved", actor="alice")
client.publish(record["logical_id"], record["version"], actor="alice")
capsule = client.get_capsule(record["logical_id"], record["version"])
bundle = client.resolve({"purpose": "privacy review", "domain": "policy", "about": "retention"})
```

No third-party dependencies — built on `urllib` since this is a small,
low-traffic client and a full HTTP stack (`requests`, etc.) would be more
than the problem calls for.

`CapsuleApiError` is raised for any non-2xx response (`.status`, `.body`);
`get_capsule` returns `None` instead of raising on a 404.

## Testing

```bash
cargo build -p capsule-api
python3 sdk-python/tests/test_client.py -v
```

Spins up a real `capsule-api` process against a temp SQLite database and
exercises the client's full register → transition → publish → get →
resolve cycle over actual HTTP, plus the 404 and validation-error paths.
Not wired into any CI in this repo — run it manually.
