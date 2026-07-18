"""The channel adapter succeeds only after the authoritative service does."""

import httpx
import pytest

from server.alerts.effects import ActionEffectError, HttpActionEffects


@pytest.mark.asyncio
async def test_authoritative_effect_completion_is_returned() -> None:
    async def handler(request: httpx.Request) -> httpx.Response:
        assert request.headers["Authorization"] == "Bearer internal-test-token"
        assert request.url.path == "/v1/actions/simulate"
        return httpx.Response(
            200, json={"status": "completed", "simulation_id": "sim-1"}
        )

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        effects = HttpActionEffects(
            "https://actions.example", "internal-test-token", client=client
        )
        result = await effects.simulate("opp-1", "actor-1")
    assert result["simulation_id"] == "sim-1"


@pytest.mark.asyncio
async def test_non_completion_fails_closed() -> None:
    async def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json={"status": "queued"})

    async with httpx.AsyncClient(transport=httpx.MockTransport(handler)) as client:
        effects = HttpActionEffects(
            "https://actions.example", "internal-test-token", client=client
        )
        with pytest.raises(ActionEffectError):
            await effects.execute_paper("opp-1", "actor-1")


def test_plaintext_remote_action_service_is_rejected() -> None:
    with pytest.raises(ValueError, match="loopback HTTP or HTTPS"):
        HttpActionEffects("http://remote.example", "token")
