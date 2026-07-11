"""MCP tier-filtering contract tests."""
import pytest
from fastapi.testclient import TestClient
from app import app

client = TestClient(app)


def test_healthz():
    resp = client.get("/healthz")
    assert resp.status_code == 200
    assert resp.json()["status"] == "ok"


def test_tier_1_only_sees_tier_1_tools():
    resp = client.get("/tools?tier=1")
    assert resp.status_code == 200
    names = {t["name"] for t in resp.json()["tools"]}
    # Tier 1 tools
    assert "brain.search" in names
    assert "markets.query" in names
    # Tier 2+ tools excluded
    assert "orders.submit_paper" not in names
    assert "orders.submit" not in names


def test_tier_3_sees_paper_not_live():
    resp = client.get("/tools?tier=3")
    names = {t["name"] for t in resp.json()["tools"]}
    assert "orders.submit_paper" in names
    assert "orders.submit" not in names


def test_tier_5_sees_all():
    resp = client.get("/tools?tier=5")
    names = {t["name"] for t in resp.json()["tools"]}
    assert "orders.submit" in names


def test_call_tool_below_tier_rejected():
    resp = client.post("/tools/orders.submit?tier=1")
    assert resp.status_code == 403


def test_call_tool_at_tier_accepted():
    resp = client.post("/tools/orders.submit_paper?tier=3")
    assert resp.status_code == 200
    assert "stub" in resp.json()["result"]


def test_unknown_tool_404():
    resp = client.post("/tools/nonexistent?tier=5")
    assert resp.status_code == 404
