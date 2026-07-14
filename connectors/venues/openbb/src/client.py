"""OpenBB platform client wrapper.

AGPL isolation note
-------------------
OpenBB is AGPL-3.0 licensed. This pack runs as a separate gRPC service so that
no OpenBB import crosses the network boundary (D7). The OpenBB library is
imported *only* within this module, inside the isolated adapter process.

See aether-blueprint/DECISIONS.md for the decision record.
"""

from __future__ import annotations

import logging
import os
import random
import threading
import time
import tomllib
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

_MANIFEST = tomllib.loads(
    (Path(__file__).resolve().parents[1] / "venue.toml").read_text(encoding="utf-8")
)
_REST_PER_MIN = int(_MANIFEST["rate_limits"]["rest_per_min"])
_BREAKER_THRESHOLD = 5
_BREAKER_RECOVERY_SECS = 30.0


class OpenbbClient:
    """Wraps the OpenBB platform SDK for equity/options data access.

    All public methods return plain Python dicts — no OpenBB types escape
    this class.

    Default provider is ``yfinance`` (free, no API key required). Override
    via the ``AETHER_VENUE__OPENBB_PROVIDER`` environment variable.
    """

    def __init__(self, obb: Any | None = None) -> None:
        self._provider: str = os.environ.get(
            "AETHER_VENUE__OPENBB_PROVIDER", "yfinance"
        )
        # Lazy-import OpenBB so that import-time failures surface here, not at
        # module load.  This also makes the module safe to import in test
        # environments without the real dependency.
        if obb is None:
            try:
                from openbb import obb as imported_obb  # type: ignore[import-untyped]
            except (ImportError, AttributeError) as exc:
                raise RuntimeError(
                    "OpenBB SDK is not installed or the local package shadows it. "
                    "Run this service from the repository root with the locked uv environment."
                ) from exc
            obb = imported_obb

        self._obb = obb
        self._budget_lock = threading.Lock()
        self._tokens = float(_REST_PER_MIN)
        self._last_refill = time.monotonic()
        self._rate_failures = 0
        self._breaker_opened_at: float | None = None
        logger.info("OpenBB client initialized (provider=%s)", self._provider)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def get_quote(self, symbol: str) -> dict[str, Any]:
        """Fetch the latest quote for a single equity symbol.

        Returns the raw OBBject results dict (or an empty dict on failure).
        """
        try:
            result = self._invoke(
                lambda: self._obb.equity.price.quote(symbol, provider=self._provider)
            )
            return self._first_dict(self._unwrap(result))
        except Exception as exc:
            logger.warning("get_quote(%s) failed: %s", symbol, exc)
            return {}

    def get_quotes(self, symbols: list[str]) -> list[dict[str, Any]]:
        """Fetch quotes for multiple equity symbols.

        OpenBB accepts comma-separated strings or Python lists.
        """
        try:
            result = self._invoke(
                lambda: self._obb.equity.price.quote(symbols, provider=self._provider)
            )
            return self._unwrap(result) or []
        except Exception as exc:
            logger.warning("get_quotes(%s) failed: %s", symbols, exc)
            return []

    def get_reference(self, symbol: str) -> dict[str, Any]:
        """Fetch company reference/profile data.

        Returns the raw OBBject results dict (or an empty dict on failure).
        """
        try:
            result = self._invoke(
                lambda: self._obb.equity.profile(symbol, provider=self._provider)
            )
            return self._first_dict(self._unwrap(result))
        except Exception as exc:
            logger.warning("get_reference(%s) failed: %s", symbol, exc)
            return {}

    def get_options_chain(
        self,
        symbol: str,
        expiration: str | None = None,
        option_type: str | None = None,
    ) -> list[dict[str, Any]]:
        """Fetch options chain for a symbol.

        Parameters
        ----------
        symbol : str
            Underlying ticker (e.g. ``"AAPL"``).
        expiration : str, optional
            ISO date string (e.g. ``"2025-09-19"``). If omitted, returns all
            available expirations.
        option_type : str, optional
            ``"call"`` or ``"put"``. If omitted, returns both.

        Returns
        -------
        list[dict]
            List of option contract dicts. Empty list on failure.
        """
        kwargs: dict[str, Any] = dict(provider=self._provider)
        if expiration is not None:
            kwargs["expiration"] = expiration
        if option_type is not None:
            kwargs["option_type"] = option_type

        try:
            result = self._invoke(
                lambda: self._obb.derivatives.options.chains(symbol, **kwargs)
            )
            raw = self._unwrap(result)
            return raw if isinstance(raw, list) else [raw] if raw else []
        except Exception as exc:
            logger.warning("get_options_chain(%s) failed: %s", symbol, exc)
            return []

    # ------------------------------------------------------------------
    # Health / connectivity check
    # ------------------------------------------------------------------

    def check_connectivity(self) -> bool:
        """Probe connectivity by fetching a quote for a well-known symbol.

        Returns ``True`` if OpenBB responds, ``False`` otherwise.
        """
        try:
            result = self._invoke(
                lambda: self._obb.equity.price.quote("SPY", provider=self._provider)
            )
            return self._unwrap(result) is not None
        except Exception:
            return False

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    @property
    def rate_remaining(self) -> int:
        """Current whole-token estimate from the manifest-sized budget."""
        with self._budget_lock:
            self._refill_locked()
            return int(self._tokens)

    def _invoke(self, call: Any) -> Any:
        """Apply the local budget and a recoverable breaker around one SDK call."""
        for attempt in range(4):
            self._acquire_token()
            try:
                result = call()
            except Exception as exc:
                text = str(exc).lower()
                if "429" not in text and "rate limit" not in text:
                    raise
                self._record_rate_failure()
                if attempt == 3:
                    raise
                ceiling = min(30.0, 0.2 * (2**attempt))
                time.sleep(random.SystemRandom().uniform(0.0, ceiling))
                continue
            self._rate_failures = 0
            self._breaker_opened_at = None
            return result
        raise RuntimeError("OpenBB retry loop exhausted")

    def _refill_locked(self) -> None:
        now = time.monotonic()
        elapsed = now - self._last_refill
        self._tokens = min(
            float(_REST_PER_MIN),
            self._tokens + elapsed * _REST_PER_MIN / 60.0,
        )
        self._last_refill = now

    def _acquire_token(self) -> None:
        while True:
            with self._budget_lock:
                if self._breaker_opened_at is not None:
                    age = time.monotonic() - self._breaker_opened_at
                    if age < _BREAKER_RECOVERY_SECS:
                        raise RuntimeError("OpenBB rate-limit circuit breaker is open")
                    self._breaker_opened_at = None
                    self._rate_failures = 0
                self._refill_locked()
                if self._tokens >= 1.0:
                    self._tokens -= 1.0
                    return
                wait = (1.0 - self._tokens) * 60.0 / _REST_PER_MIN
            time.sleep(wait)

    def _record_rate_failure(self) -> None:
        with self._budget_lock:
            self._rate_failures += 1
            if self._rate_failures >= _BREAKER_THRESHOLD:
                self._breaker_opened_at = time.monotonic()

    @staticmethod
    def _unwrap(obbject: Any) -> dict[str, Any] | list[dict[str, Any]] | None:
        """Unwrap an ``OBBject`` to its ``.results``.

        OpenBB 4.x returns ``OBBject`` instances. The real data lives in
        ``.results`` (a list or a single object). This helper extracts it
        consistently.
        """
        if obbject is None:
            return None
        results = getattr(obbject, "results", None)
        if results is None:
            return None
        # results may be a list of dicts, a single dict, or None
        if isinstance(results, list):
            return [r for r in results if r is not None] or None
        if isinstance(results, dict):
            return results
        # Fallback: try to_dict()
        if hasattr(results, "to_dict"):
            return results.to_dict()
        return None

    @staticmethod
    def _first_dict(
        value: dict[str, Any] | list[dict[str, Any]] | None,
    ) -> dict[str, Any]:
        """Return the first mapping from a single-record SDK operation."""
        if isinstance(value, dict):
            return value
        if isinstance(value, list) and value:
            return value[0]
        return {}
