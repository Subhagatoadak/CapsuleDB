/**
 * A thin HTTP client for capsule-api (Phase 5 SDK).
 *
 * Built on the platform `fetch` (Node 18+, and every browser) rather
 * than an HTTP client library — no dependency to install. It is
 * deliberately a 1:1 mapping onto capsule-api's routes: no retries, no
 * connection pooling, no auth beyond what you pass in `fetchOptions`
 * yourself (capsule-api has none of its own yet either).
 */

export interface AccessOptions {
  consumerRole?: string;
  canReadEvidence?: boolean;
}

export class CapsuleApiError extends Error {
  status: number;
  body: unknown;

  constructor(status: number, body: unknown) {
    super(`capsule-api returned ${status}: ${errorMessage(body)}`);
    this.status = status;
    this.body = body;
  }
}

function errorMessage(body: unknown): string {
  if (body && typeof body === "object" && "error" in body) {
    return String((body as { error: unknown }).error);
  }
  return String(body);
}

export class CapsuleClient {
  private readonly baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  async health(): Promise<{ status: string }> {
    return this.request("GET", "/health");
  }

  async getCapsule(logicalId: string, version: string, access: AccessOptions = {}): Promise<Record<string, unknown> | null> {
    const params: Record<string, string> = { logical_id: logicalId, version };
    if (access.consumerRole) params.consumer_role = access.consumerRole;
    if (access.canReadEvidence) params.can_read_evidence = "true";

    try {
      return await this.request("GET", "/capsule", params);
    } catch (err) {
      if (err instanceof CapsuleApiError && err.status === 404) return null;
      throw err;
    }
  }

  async listVersions(logicalId: string): Promise<unknown[]> {
    return this.request("GET", "/versions", { logical_id: logicalId });
  }

  async resolve(request: Record<string, unknown>, consumerRole?: string): Promise<Record<string, unknown>> {
    const params = consumerRole ? { consumer_role: consumerRole } : undefined;
    return this.request("POST", "/resolve", params, request);
  }

  async register(capsule: Record<string, unknown>, actor: string): Promise<Record<string, unknown>> {
    return this.request("POST", "/register", { actor }, capsule);
  }

  async transition(logicalId: string, version: string, to: string, actor: string, reason?: string): Promise<Record<string, unknown>> {
    const params: Record<string, string> = { logical_id: logicalId, version, to, actor };
    if (reason) params.reason = reason;
    return this.request("POST", "/transition", params);
  }

  async publish(logicalId: string, version: string, actor: string): Promise<Record<string, unknown>> {
    return this.request("POST", "/publish", { logical_id: logicalId, version, actor });
  }

  private async request(
    method: string,
    path: string,
    params?: Record<string, string>,
    body?: Record<string, unknown>,
  ): Promise<any> {
    let url = this.baseUrl + path;
    if (params) {
      url += "?" + new URLSearchParams(params).toString();
    }

    const response = await fetch(url, {
      method,
      headers: { "Content-Type": "application/json" },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });

    const text = await response.text();
    const parsed = text.length > 0 ? JSON.parse(text) : null;

    if (!response.ok) {
      throw new CapsuleApiError(response.status, parsed);
    }
    return parsed;
  }
}
