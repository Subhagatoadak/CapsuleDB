// Smoke test for CapsuleClient against a real capsule-api process.
// Not wired into any CI here — run manually:
//
//   cargo build -p capsule-api
//   node --test sdk-typescript/tests/smoke.ts
//
// Requires Node 22+ (native TypeScript execution, no build step) and
// expects target/debug/capsule-api to already exist.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn, ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { CapsuleClient, CapsuleApiError } from "../src/client.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(__dirname, "..", "..");
const BINARY = join(REPO_ROOT, "target", "debug", "capsule-api");
const PORT = 18090;

let process_: ChildProcess;
let tmpDir: string;
const client = new CapsuleClient(`http://127.0.0.1:${PORT}`);

function draftCapsule(logicalId: string) {
  return {
    schema_version: "kcp/0.1",
    logical_id: logicalId,
    version: "1.0.0",
    capsule_type: "rule",
    statement: "Retain data for 90 days.",
    epistemic_status: "policy_defined",
    confidence: { basis: "policy_mandate", ceiling: 0.9 },
    lifecycle_state: "draft",
    governance: { owner: "alice", domain: "policy" },
  };
}

before(async () => {
  tmpDir = mkdtempSync(join(tmpdir(), "capsuledb-ts-"));
  const dbPath = join(tmpDir, "capsules.sqlite3");
  process_ = spawn(BINARY, ["--db", dbPath, "--addr", `127.0.0.1:${PORT}`], { stdio: "ignore" });

  for (let i = 0; i < 50; i++) {
    try {
      await client.health();
      return;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
  throw new Error("capsule-api did not become healthy in time");
});

after(() => {
  process_.kill();
  rmSync(tmpDir, { recursive: true, force: true });
});

test("health check", async () => {
  const health = await client.health();
  assert.equal(health.status, "ok");
});

test("full lifecycle and resolve", async () => {
  const logicalId = "kc://policy/retention/ts-sdk-test";
  const record = await client.register(draftCapsule(logicalId), "alice");
  assert.equal(record.lifecycle_state, "draft");

  await client.transition(logicalId, "1.0.0", "in_review", "alice");
  await client.transition(logicalId, "1.0.0", "approved", "alice");
  const outcome = await client.publish(logicalId, "1.0.0", "alice");
  assert.ok("outcomes" in outcome);

  const capsule = await client.getCapsule(logicalId, "1.0.0");
  assert.equal(capsule?.lifecycle_state, "published");

  const versions = await client.listVersions(logicalId);
  assert.equal(versions.length, 1);

  const bundle = await client.resolve({ purpose: "check", domain: "policy", about: "retain" });
  assert.equal((bundle.capsules as unknown[]).length, 1);
});

test("get missing capsule returns null", async () => {
  const capsule = await client.getCapsule("kc://a/b/c", "1.0.0");
  assert.equal(capsule, null);
});

test("register invalid capsule throws CapsuleApiError", async () => {
  await assert.rejects(
    () => client.register(draftCapsule("NOT-VALID"), "alice"),
    (err: unknown) => err instanceof CapsuleApiError && err.status === 400,
  );
});
