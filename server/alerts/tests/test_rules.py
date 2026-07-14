"""Tests for the alert rule engine (dedup, rate-limit, filters)."""

import pytest

from server.alerts.rules import DEFAULT_RULES, Rule, _rate_limiters, evaluate

# ── Helpers ───────────────────────────────────────────────────────────────


def _opp(
    opp_id: str = "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    kind: str = "arbitrage",
    net_edge: str = "0.05",
    confidence: float = 0.85,
    venue: str = "kalshi",
    market: str = "mkt:kalshi:BTC-75",
    trace_id: str = "trace-001",
) -> dict:
    return {
        "id": opp_id,
        "kind": kind,
        "net_edge": net_edge,
        "confidence": confidence,
        "venue": venue,
        "market": market,
        "trace_id": trace_id,
    }


# ── Tests ─────────────────────────────────────────────────────────────────


class TestRuleMatching:
    """Filter-based matching tests."""

    @pytest.mark.asyncio
    async def test_rule_matches_arbitrage(self) -> None:
        """Arbitrage with net_edge > threshold matches high_edge_arb rule."""
        opp = _opp(kind="arbitrage", net_edge="0.05", confidence=0.85)
        rule = Rule(
            name="high_edge_arb",
            kind_filter=["arbitrage"],
            min_net_edge="0.03",
            min_confidence=0.7,
        )
        results = await evaluate(opp, [rule])
        assert len(results) == 1
        assert results[0][0].name == "high_edge_arb"

    @pytest.mark.asyncio
    async def test_rule_does_not_match_wrong_kind(self) -> None:
        """Value opp doesn't match arbitrage-only rule."""
        opp = _opp(kind="value", net_edge="0.05", confidence=0.85)
        rule = Rule(name="arb_only", kind_filter=["arbitrage"])
        results = await evaluate(opp, [rule])
        assert len(results) == 0

    @pytest.mark.asyncio
    async def test_rule_confidence_filter(self) -> None:
        """Opp below min_confidence doesn't match."""
        opp = _opp(kind="arbitrage", net_edge="0.05", confidence=0.5)
        rule = Rule(name="high_conf", kind_filter=["arbitrage"], min_confidence=0.7)
        results = await evaluate(opp, [rule])
        assert len(results) == 0

    @pytest.mark.asyncio
    async def test_rule_venue_filter(self) -> None:
        """Wrong venue doesn't match."""
        opp = _opp(kind="arbitrage", venue="polymarket")
        rule = Rule(
            name="kalshi_only", kind_filter=["arbitrage"], venue_filter=["kalshi"]
        )
        results = await evaluate(opp, [rule])
        assert len(results) == 0

    @pytest.mark.asyncio
    async def test_rule_net_edge_below_threshold(self) -> None:
        """Opp below min_net_edge doesn't match."""
        opp = _opp(kind="arbitrage", net_edge="0.01")
        rule = Rule(name="high_edge", kind_filter=["arbitrage"], min_net_edge="0.03")
        results = await evaluate(opp, [rule])
        assert len(results) == 0

    @pytest.mark.asyncio
    async def test_multiple_rules_match(self) -> None:
        """One opp can match multiple rules."""
        opp = _opp(kind="arbitrage", net_edge="0.05", confidence=0.85)
        rule_a = Rule(name="rule_a", kind_filter=["arbitrage"])
        rule_b = Rule(name="rule_b", kind_filter=["arbitrage"])
        results = await evaluate(opp, [rule_a, rule_b])
        assert len(results) == 2
        names = {r[0].name for r in results}
        assert names == {"rule_a", "rule_b"}

    @pytest.mark.asyncio
    async def test_market_filter(self) -> None:
        """Market filter excludes non-matching opportunities."""
        opp = _opp(kind="arbitrage", market="mkt:polymarket:ETH-100")
        rule = Rule(name="btc_only", market_filter=["mkt:kalshi:BTC-75"])
        results = await evaluate(opp, [rule])
        assert len(results) == 0

    @pytest.mark.asyncio
    async def test_market_filter_matches(self) -> None:
        """Market filter passes when market matches."""
        opp = _opp(kind="arbitrage", market="mkt:kalshi:BTC-75")
        rule = Rule(name="btc_only", market_filter=["mkt:kalshi:BTC-75"])
        results = await evaluate(opp, [rule])
        assert len(results) == 1
        assert results[0][0].name == "btc_only"


class TestDedup:
    """Deduplication tests."""

    @pytest.mark.asyncio
    async def test_dedup(self) -> None:
        """Same (opp_id, rule_name) seen twice — second skipped."""
        opp = _opp()
        rule = Rule(name="test_rule", kind_filter=["arbitrage"])
        results1 = await evaluate(opp, [rule])
        assert len(results1) == 1

        results2 = await evaluate(opp, [rule])
        assert len(results2) == 0

    @pytest.mark.asyncio
    async def test_dedup_different_opp_ids(self) -> None:
        """Different opp IDs with same rule are NOT dedup'd."""
        opp1 = _opp(opp_id="AAA")
        opp2 = _opp(opp_id="BBB")
        rule = Rule(name="test_rule", kind_filter=["arbitrage"])
        results1 = await evaluate(opp1, [rule])
        results2 = await evaluate(opp2, [rule])
        assert len(results1) == 1
        assert len(results2) == 1

    @pytest.mark.asyncio
    async def test_dedup_different_rules(self) -> None:
        """Same opp ID with different rules are NOT dedup'd."""
        opp = _opp()
        rule_a = Rule(name="rule_a", kind_filter=["arbitrage"])
        rule_b = Rule(name="rule_b", kind_filter=["arbitrage"])
        results1 = await evaluate(opp, [rule_a])
        results2 = await evaluate(opp, [rule_b])
        assert len(results1) == 1
        assert len(results2) == 1


class TestRateLimit:
    """Rate-limiting tests."""

    @pytest.mark.asyncio
    async def test_rate_limit(self) -> None:
        """N+1 alerts within a minute — N+1th skipped."""
        rule = Rule(name="rate_limited", rate_limit_per_minute=1)

        opp1 = _opp(opp_id="AAA")
        opp2 = _opp(opp_id="BBB")

        results1 = await evaluate(opp1, [rule])
        assert len(results1) == 1  # first alert allowed

        results2 = await evaluate(opp2, [rule])
        assert len(results2) == 0  # rate-limited

    @pytest.mark.asyncio
    async def test_rate_limit_resets_after_window(self) -> None:
        """Sliding window resets after time passes (simulated)."""
        rule = Rule(name="rate_limited", rate_limit_per_minute=1)

        # First alert fills the window
        results1 = await evaluate(_opp(opp_id="AAA"), [rule])
        assert len(results1) == 1

        # Manually clear the deque to simulate the window having passed
        _rate_limiters[rule.name].clear()

        # Second alert should now pass
        results2 = await evaluate(_opp(opp_id="BBB"), [rule])
        assert len(results2) == 1

    @pytest.mark.asyncio
    async def test_rate_limit_per_rule(self) -> None:
        """Each rule has its own rate-limit counter."""
        rule_a = Rule(name="rule_a", rate_limit_per_minute=1)
        rule_b = Rule(name="rule_b", rate_limit_per_minute=2)

        opp1 = _opp(opp_id="AAA")
        opp2 = _opp(opp_id="BBB")
        opp3 = _opp(opp_id="CCC")

        # First opp hits both rules
        r = await evaluate(opp1, [rule_a, rule_b])
        assert len(r) == 2

        # Second opp: rule_a is rate-limited (1/min), rule_b passes
        r = await evaluate(opp2, [rule_a, rule_b])
        assert len(r) == 1
        assert r[0][0].name == "rule_b"

        # Third opp: rule_a still rate-limited, rule_b now also rate-limited (2/min)
        r = await evaluate(opp3, [rule_a, rule_b])
        assert len(r) == 0


class TestDefaultRules:
    """DEFAULT_RULES fixture tests."""

    @pytest.mark.asyncio
    async def test_default_rules_present(self) -> None:
        """DEFAULT_RULES has expected entries."""
        assert len(DEFAULT_RULES) == 3
        names = [r.name for r in DEFAULT_RULES]
        assert "high_edge_arb" in names
        assert "catalyst_events" in names
        assert "all_opportunities" in names

    @pytest.mark.asyncio
    async def test_high_edge_arb_config(self) -> None:
        """high_edge_arb has correct default config."""
        rule = next(r for r in DEFAULT_RULES if r.name == "high_edge_arb")
        assert rule.kind_filter == ["arbitrage"]
        assert rule.min_net_edge == "0.03"
        assert rule.min_confidence == 0.7

    @pytest.mark.asyncio
    async def test_default_rules_match_appropriate_opps(self) -> None:
        """DEFAULT_RULES only match opportunities that pass their filters."""
        # An arb opp meeting all thresholds should match at least high_edge_arb
        arb_opp = _opp(
            opp_id="OPP-001", kind="arbitrage", net_edge="0.05", confidence=0.85
        )
        results = await evaluate(arb_opp)
        matched_names = {r[0].name for r in results}
        assert "high_edge_arb" in matched_names
        assert "all_opportunities" in matched_names

        # A different opp should NOT match high_edge_arb or catalyst_events
        # but SHOULD match all_opportunities (no kind filter)
        value_opp = _opp(
            opp_id="OPP-002", kind="value", net_edge="0.05", confidence=0.85
        )
        results2 = await evaluate(value_opp)
        matched_names2 = {r[0].name for r in results2}
        assert "high_edge_arb" not in matched_names2
        assert "catalyst_events" not in matched_names2
        assert "all_opportunities" in matched_names2
