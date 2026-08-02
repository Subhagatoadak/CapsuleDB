"""A thin HTTP client for capsule-api (Phase 5 SDK).

Uses only the standard library — no `requests` dependency — since this
is a small, low-traffic client and pulling in a third-party HTTP stack
for it would be more than the problem calls for. It is deliberately a
1:1 mapping onto capsule-api's routes; it does not implement retries,
connection pooling, or auth beyond what you pass through headers
yourself (capsule-api has none of its own yet either — see
`capsule-api`'s docs for the current trust model).
"""

from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Optional


class CapsuleApiError(Exception):
    """Raised for any non-2xx response, carrying the status and parsed body."""

    def __init__(self, status: int, body: Any):
        self.status = status
        self.body = body
        message = body.get("error") if isinstance(body, dict) else body
        super().__init__(f"capsule-api returned {status}: {message}")


class CapsuleClient:
    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip("/")

    def health(self) -> dict:
        return self._request("GET", "/health")

    def get_capsule(
        self,
        logical_id: str,
        version: str,
        consumer_role: Optional[str] = None,
        can_read_evidence: bool = False,
    ) -> Optional[dict]:
        params = {"logical_id": logical_id, "version": version}
        if consumer_role:
            params["consumer_role"] = consumer_role
        if can_read_evidence:
            params["can_read_evidence"] = "true"
        try:
            return self._request("GET", "/capsule", params=params)
        except CapsuleApiError as e:
            if e.status == 404:
                return None
            raise

    def list_versions(self, logical_id: str) -> list:
        return self._request("GET", "/versions", params={"logical_id": logical_id})

    def resolve(self, request: dict, consumer_role: Optional[str] = None) -> dict:
        params = {"consumer_role": consumer_role} if consumer_role else None
        return self._request("POST", "/resolve", params=params, body=request)

    def register(self, capsule: dict, actor: str) -> dict:
        return self._request("POST", "/register", params={"actor": actor}, body=capsule)

    def transition(self, logical_id: str, version: str, to: str, actor: str, reason: Optional[str] = None) -> dict:
        params = {"logical_id": logical_id, "version": version, "to": to, "actor": actor}
        if reason:
            params["reason"] = reason
        return self._request("POST", "/transition", params=params)

    def publish(self, logical_id: str, version: str, actor: str) -> dict:
        params = {"logical_id": logical_id, "version": version, "actor": actor}
        return self._request("POST", "/publish", params=params)

    def _request(self, method: str, path: str, params: Optional[dict] = None, body: Optional[dict] = None) -> Any:
        url = self.base_url + path
        if params:
            url += "?" + urllib.parse.urlencode(params)

        data = json.dumps(body).encode("utf-8") if body is not None else None
        req = urllib.request.Request(url, data=data, method=method, headers={"Content-Type": "application/json"})

        try:
            with urllib.request.urlopen(req) as response:
                raw = response.read()
                return json.loads(raw) if raw else None
        except urllib.error.HTTPError as e:
            raw = e.read()
            parsed = json.loads(raw) if raw else {"error": str(e)}
            raise CapsuleApiError(e.code, parsed) from None
