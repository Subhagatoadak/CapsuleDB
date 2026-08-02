"""Smoke test for capsuledb.CapsuleClient against a real capsule-api
process. Not wired into any CI here — run manually:

    cargo build -p capsule-api
    python3 sdk-python/tests/test_client.py

It builds nothing itself; it expects `target/debug/capsule-api` to
already exist (any `cargo build`, `cargo test`, or `cargo run` touching
that crate produces it).
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from capsuledb import CapsuleApiError, CapsuleClient  # noqa: E402

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
BINARY = REPO_ROOT / "target" / "debug" / "capsule-api"


def draft_capsule(logical_id: str) -> dict:
    return {
        "schema_version": "kcp/0.1",
        "logical_id": logical_id,
        "version": "1.0.0",
        "capsule_type": "rule",
        "statement": "Retain data for 90 days.",
        "epistemic_status": "policy_defined",
        "confidence": {"basis": "policy_mandate", "ceiling": 0.9},
        "lifecycle_state": "draft",
        "governance": {"owner": "alice", "domain": "policy"},
    }


class ClientSmokeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BINARY.exists():
            raise unittest.SkipTest(f"{BINARY} not built; run `cargo build -p capsule-api` first")

        cls.tmpdir = tempfile.TemporaryDirectory()
        db_path = pathlib.Path(cls.tmpdir.name) / "capsules.sqlite3"
        cls.port = 18089
        cls.process = subprocess.Popen(
            [str(BINARY), "--db", str(db_path), "--addr", f"127.0.0.1:{cls.port}"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        cls.client = CapsuleClient(f"http://127.0.0.1:{cls.port}")

        for _ in range(50):
            try:
                cls.client.health()
                break
            except Exception:
                time.sleep(0.1)
        else:
            raise RuntimeError("capsule-api did not become healthy in time")

    @classmethod
    def tearDownClass(cls):
        cls.process.terminate()
        cls.process.wait(timeout=5)
        cls.tmpdir.cleanup()

    def test_health(self):
        self.assertEqual(self.client.health()["status"], "ok")

    def test_full_lifecycle_and_resolve(self):
        logical_id = "kc://policy/retention/python-sdk-test"
        record = self.client.register(draft_capsule(logical_id), actor="alice")
        self.assertEqual(record["lifecycle_state"], "draft")

        self.client.transition(logical_id, "1.0.0", "in_review", actor="alice")
        self.client.transition(logical_id, "1.0.0", "approved", actor="alice")
        outcome = self.client.publish(logical_id, "1.0.0", actor="alice")
        self.assertIn("outcomes", outcome)

        capsule = self.client.get_capsule(logical_id, "1.0.0")
        self.assertEqual(capsule["lifecycle_state"], "published")

        versions = self.client.list_versions(logical_id)
        self.assertEqual(len(versions), 1)

        bundle = self.client.resolve({"purpose": "check", "domain": "policy", "about": "retain"})
        self.assertEqual(len(bundle["capsules"]), 1)

    def test_get_missing_capsule_returns_none(self):
        self.assertIsNone(self.client.get_capsule("kc://a/b/c", "1.0.0"))

    def test_register_invalid_capsule_raises(self):
        bad = draft_capsule("NOT-VALID")
        with self.assertRaises(CapsuleApiError) as ctx:
            self.client.register(bad, actor="alice")
        self.assertEqual(ctx.exception.status, 400)


if __name__ == "__main__":
    unittest.main()
