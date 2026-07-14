"""Rule engine — evaluate opportunities against configurable rules."""

import logging
import time
from collections import defaultdict, deque
from dataclasses import dataclass, field
from decimal import Decimal

logger = logging.getLogger(__name__)

# ── In-memory dedup (opportunity_id, rule_name) — TTL 1 hour ──────────
_dedup_set: set[tuple[str, str]] = set()
_dedup_timestamps: dict[tuple[str, str], float] = {}

# ── In-memory rate limiter (sliding window per rule) ──────────────────
_rate_limiters: dict[str, deque[float]] = defaultdict(deque)


@dataclass
class Rule:
    """A single alert rule with filters and rate-limit settings."""

    name: str
    kind_filter: list[str] | None = None
    min_net_edge: str | None = None  # e.g. "0.03" (3%)
    min_confidence: float | None = None  # e.g. 0.7
    venue_filter: list[str] | None = None
    market_filter: list[str] | None = None
    channels: list[str] = field(
        default_factory=lambda: ["telegram", "discord", "slack"]
    )
    rate_limit_per_minute: int = 5


DEFAULT_RULES = [
    Rule(
        name="high_edge_arb",
        kind_filter=["arbitrage"],
        min_net_edge="0.03",
        min_confidence=0.7,
    ),
    Rule(
        name="catalyst_events",
        kind_filter=["catalyst"],
        min_confidence=0.8,
    ),
    Rule(
        name="all_opportunities",
        rate_limit_per_minute=2,
    ),
]


def _check_dedup(opportunity_id: str, rule_name: str) -> bool:
    """Return True if *(opportunity_id, rule_name)* has already been seen.

    Entries older than 1 hour are pruned before checking.
    """
    _prune_dedup()
    key = (opportunity_id, rule_name)
    if key in _dedup_set:
        return True
    _dedup_set.add(key)
    _dedup_timestamps[key] = time.time()
    return False


def _prune_dedup() -> None:
    """Remove dedup entries older than 1 hour."""
    now = time.time()
    cutoff = now - 3600
    stale = [k for k, t in _dedup_timestamps.items() if t < cutoff]
    for k in stale:
        _dedup_set.discard(k)
        del _dedup_timestamps[k]


def _check_rate_limit(rule: Rule) -> bool:
    """Return True if sending *rule* would exceed its per-minute rate limit."""
    now = time.time()
    window = 60.0
    rl = _rate_limiters[rule.name]

    # Purge stale entries
    while rl and rl[0] < now - window:
        rl.popleft()

    if len(rl) >= rule.rate_limit_per_minute:
        return True

    rl.append(now)
    return False


def _reset_state() -> None:
    """Reset dedup and rate-limit state (for testing)."""
    _dedup_set.clear()
    _dedup_timestamps.clear()
    _rate_limiters.clear()


async def evaluate(
    opportunity: dict,
    rules: list[Rule] | None = None,
) -> list[tuple[Rule, str]]:
    """Evaluate *opportunity* against *rules*.

    Returns a list of ``(matching_rule, human-readable_reason)`` tuples for
    every rule whose filters (kind, net-edge, confidence, venue, market) all
    pass **and** whose dedup / rate-limit checks pass.

    When *rules* is ``None``, the ``DEFAULT_RULES`` list is used.
    """
    if rules is None:
        rules = DEFAULT_RULES

    opportunity_id = opportunity.get("id", "")
    opportunity_kind = opportunity.get("kind", "")
    net_edge = opportunity.get("net_edge", "0")
    confidence = opportunity.get("confidence", 1.0)
    venue = opportunity.get("venue", "")
    market = opportunity.get("market", "")

    results: list[tuple[Rule, str]] = []

    for rule in rules:
        # ── Kind filter ────────────────────────────────────────────
        if rule.kind_filter is not None and opportunity_kind not in rule.kind_filter:
            continue

        # ── Net-edge filter ────────────────────────────────────────
        if rule.min_net_edge is not None:
            try:
                if Decimal(net_edge) < Decimal(rule.min_net_edge):
                    continue
            except Exception:
                continue

        # ── Confidence filter ──────────────────────────────────────
        if rule.min_confidence is not None and confidence < rule.min_confidence:
            continue

        # ── Venue filter ───────────────────────────────────────────
        if rule.venue_filter is not None and venue not in rule.venue_filter:
            continue

        # ── Market filter ──────────────────────────────────────────
        if rule.market_filter is not None and market not in rule.market_filter:
            continue

        # ── Dedup ──────────────────────────────────────────────────
        if _check_dedup(opportunity_id, rule.name):
            logger.debug("dedup hit: opp=%s rule=%s", opportunity_id, rule.name)
            continue

        # ── Rate limit ─────────────────────────────────────────────
        if _check_rate_limit(rule):
            logger.debug("rate-limited: rule=%s", rule.name)
            continue

        reason = f"matched rule '{rule.name}' for kind='{opportunity_kind}'"
        if rule.kind_filter is not None:
            reason += f" (kind_filter={rule.kind_filter})"
        results.append((rule, reason))

    return results
