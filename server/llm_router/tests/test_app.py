"""Tests for the LLM Router FastAPI endpoints."""

from __future__ import annotations

from unittest.mock import patch

import pytest
from fastapi.testclient import TestClient

from server.llm_router.app import app


@pytest.fixture
def client():
    """Return a TestClient for the FastAPI app."""
    with TestClient(app) as c:
        yield c


class TestHealth:
    """GET /healthz"""

    def test_healthz_returns_200(self, client):
        resp = client.get("/healthz")
        assert resp.status_code == 200
        assert resp.json() == {"status": "ok", "service": "llm_router"}


class TestComplete:
    """POST /complete"""

    @patch("server.llm_router.app.complete_with_fallback")
    def test_complete_valid_request(self, mock_complete, client):
        """A valid request returns 200 with the expected shape."""
        mock_complete.return_value = {
            "text": "mock result",
            "usage": {"prompt_tokens": 10, "completion_tokens": 20},
            "model": "claude-haiku-4-5-20251001",
            "provider": "anthropic",
            "cache_hit": False,
            "cost_usd": 0.001,
        }

        payload = {
            "purpose": "summarize",
            "dynamic_inputs": {"user_text": "Hello"},
            "model_policy": "default",
        }
        resp = client.post("/complete", json=payload)
        assert resp.status_code == 200

        body = resp.json()
        assert body["text"] == "mock result"
        assert body["usage"]["prompt_tokens"] == 10
        assert body["provider"] == "anthropic"

    def test_complete_invalid_request(self, client):
        """An invalid request (missing required fields) returns 422."""
        resp = client.post("/complete", json={})
        assert resp.status_code == 422

    @patch("server.llm_router.app.complete_with_fallback")
    def test_complete_unknown_purpose_returns_422(self, mock_complete, client):
        """An unknown purpose propagated from the router returns 422."""
        # The router raises ValueError → the endpoint catches it as 422
        mock_complete.side_effect = ValueError("Unknown purpose 'fake'")

        payload = {
            "purpose": "fake",
            "dynamic_inputs": {"user_text": "Hello"},
        }
        resp = client.post("/complete", json=payload)
        assert resp.status_code == 422
        assert "Unknown purpose" in resp.json()["detail"]
