# sdk-typescript

A thin, dependency-free HTTP client for `capsule-api` (Phase 5), built on
the platform `fetch` (Node 18+, and every browser).

```ts
import { CapsuleClient } from "./src/client.ts";

const client = new CapsuleClient("http://127.0.0.1:8089");
await client.health();
const record = await client.register(capsuleJson, "alice");
await client.transition(record.logical_id as string, record.version as string, "in_review", "alice");
await client.transition(record.logical_id as string, record.version as string, "approved", "alice");
await client.publish(record.logical_id as string, record.version as string, "alice");
const capsule = await client.getCapsule(record.logical_id as string, record.version as string);
const bundle = await client.resolve({ purpose: "privacy review", domain: "policy", about: "retention" });
```

Ships as plain TypeScript source with no build step required: Node 22+
runs `.ts` files directly (type-stripping). `tsconfig.json` is provided
for consumers who want a real `tsc`/bundler build with type-checking; it
is not required to run the SDK or its tests.

`CapsuleApiError` is thrown for any non-2xx response (`.status`, `.body`);
`getCapsule` returns `null` instead of throwing on a 404.

## Testing

```bash
cargo build -p capsule-api
node --test sdk-typescript/tests/smoke.ts
```

Spins up a real `capsule-api` process against a temp SQLite database and
exercises the client's full register → transition → publish → get →
resolve cycle over actual HTTP, plus the 404 and validation-error paths.
Not wired into any CI in this repo — run it manually.
