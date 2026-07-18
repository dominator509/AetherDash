"""Fail-closed client for the authoritative server-plane action service."""

from __future__ import annotations

import os
from typing import Any

import httpx


class ActionEffectError(RuntimeError):
    """An authorized action did not complete through the normal path."""


class HttpActionEffects:
    """Invoke existing server-plane action endpoints without reimplementing policy.

    The target service must independently load the actor's current grant and run
    the canonical simulator/router checks.  A channel callback is never itself
    treated as authority.
    """

    def __init__(
        self,
        base_url: str,
        service_token: str,
        *,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        if not base_url.startswith(
            ("http://127.0.0.1", "http://localhost", "https://")
        ):
            raise ValueError("action service URL must be loopback HTTP or HTTPS")
        if not service_token:
            raise ValueError("action service token is required")
        self._base_url = base_url.rstrip("/")
        self._service_token = service_token
        self._client = client

    async def _invoke(
        self,
        action: str,
        opportunity_id: str,
        actor_id: str,
        approval_id: str | None = None,
    ) -> dict[str, Any]:
        payload = {"opportunity_id": opportunity_id, "actor_id": actor_id}
        if approval_id is not None:
            payload["approval_id"] = approval_id
        headers = {"Authorization": f"Bearer {self._service_token}"}
        owns_client = self._client is None
        client = self._client or httpx.AsyncClient(timeout=15, follow_redirects=False)
        try:
            response = await client.post(
                f"{self._base_url}/v1/actions/{action}", json=payload, headers=headers
            )
            response.raise_for_status()
            result = response.json()
        except (httpx.HTTPError, ValueError) as exc:
            raise ActionEffectError(
                "authoritative action service rejected the request"
            ) from exc
        finally:
            if owns_client:
                await client.aclose()
        if not isinstance(result, dict) or result.get("status") != "completed":
            raise ActionEffectError(
                "authoritative action service did not confirm completion"
            )
        return result

    async def simulate(self, opportunity_id: str, actor_id: str) -> dict[str, Any]:
        return await self._invoke("simulate", opportunity_id, actor_id)

    async def ignore(self, opportunity_id: str, actor_id: str) -> dict[str, Any]:
        return await self._invoke("ignore", opportunity_id, actor_id)

    async def execute_paper(
        self, opportunity_id: str, actor_id: str, approval_id: str | None = None
    ) -> dict[str, Any]:
        return await self._invoke(
            "execute-paper", opportunity_id, actor_id, approval_id
        )

    async def execute_live(
        self, opportunity_id: str, actor_id: str, approval_id: str
    ) -> dict[str, Any]:
        return await self._invoke("execute-live", opportunity_id, actor_id, approval_id)

    async def approve_guardian(
        self, proposal_id: str, actor_id: str, approval_id: str
    ) -> dict[str, Any]:
        return await self._invoke(
            "guardian-approve", proposal_id, actor_id, approval_id
        )


def action_effects_from_env() -> HttpActionEffects | None:
    """Build the adapter only when both transport settings are present."""
    base_url = os.environ.get("AETHER_ALERTS_ACTIONS_URL", "")
    token = os.environ.get("AETHER_ALERTS_ACTIONS_TOKEN", "")
    if not base_url or not token:
        return None
    return HttpActionEffects(base_url, token)
