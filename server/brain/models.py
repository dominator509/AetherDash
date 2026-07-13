"""Brain object model — all SPEC-011 fields as Pydantic models."""

import hashlib
from datetime import UTC, datetime
from enum import StrEnum

from aether_py.canonical import canonical_json_string
from pydantic import BaseModel, field_validator

# ── Enums ──────────────────────────────────────────────────────────────────


class ObjectKind(StrEnum):
    document = "document"
    email = "email"
    filing = "filing"
    news = "news"
    note = "note"
    market_description = "market_description"
    event = "event"
    screenshot = "screenshot"
    report = "report"
    transcript = "transcript"


class Origin(StrEnum):
    ingest_fleet = "ingest_fleet"
    inbox = "inbox"
    operator = "operator"
    system = "system"


class TrustLevel(StrEnum):
    low = "low"
    medium = "medium"
    high = "high"


class Tier(StrEnum):
    hot = "hot"
    warm = "warm"
    cold = "cold"


# ── Domain models ─────────────────────────────────────────────────────────


class BrainRef(BaseModel):
    """Identifier returned by Brain.Store."""

    id: str  # ULID
    provenance_hash: str


class ObjectDraft(BaseModel):
    """Proto mirror: the input to Brain.Store."""

    kind: str
    content: str
    source: str


class BrainObject(BaseModel):
    """Full Brain object with all SPEC-011 required fields."""

    id: str  # ULID
    kind: ObjectKind
    source: str
    origin: Origin
    trust: TrustLevel = TrustLevel.medium
    author_or_publisher: str | None = None
    published_ts: str | None = None  # ISO 8601
    ingested_ts: str  # ISO 8601
    url_or_ref: str | None = None
    raw_ref: str | None = None  # MinIO aether-raw sha256 key
    clean_ref: str | None = None  # MinIO aether-clean sha256 key
    provenance_hash: str
    summary: str | None = None
    entities: list[str] = []
    linked_events: list[str] = []
    market_keys: list[str] = []
    confidence: float | None = None  # 0..1
    staleness_rule: str | None = None
    expires_ts: str | None = None  # ISO 8601
    tier: Tier = Tier.warm
    current_stage: str = "intake"
    parked_reason: str | None = None

    @field_validator("id")
    @classmethod
    def valid_ulid(cls, v: str) -> str:
        if len(v) != 26:
            raise ValueError(f"ULID must be 26 chars, got {len(v)}")
        return v


# ── Provenance helpers ────────────────────────────────────────────────────


class ProvenancePayload(BaseModel):
    """Canonical payload for provenance hashing: {source, raw_sha256, ingested_ts}."""

    source: str
    raw_sha256: str
    ingested_ts: str


def compute_provenance_hash(source: str, raw_sha256: str, ingested_ts: str) -> str:
    """Compute SHA-256 over canonical JSON of {source, raw_sha256, ingested_ts}.

    This matches Rust ``aether-core`` canonical bytes for the same triple.
    Uses the shared ``canonical_json_string`` from ``aether_py`` for key-sorted,
    no-whitespace JSON that is byte-identical across languages.
    """
    payload = ProvenancePayload(
        source=source, raw_sha256=raw_sha256, ingested_ts=ingested_ts
    )
    canonical = payload.model_dump(mode="json", exclude_defaults=True)
    canonical_str = canonical_json_string(canonical)
    return hashlib.sha256(canonical_str.encode()).hexdigest()


def now_iso() -> str:
    """Return current UTC time as ISO 8601 with millisecond precision."""
    now = datetime.now(UTC)
    ms = now.microsecond // 1000
    return now.strftime(f"%Y-%m-%dT%H:%M:%S.{ms:03d}Z")


# ── Trust-level mapping ───────────────────────────────────────────────────


def trust_to_numeric(t: TrustLevel) -> float:
    mapping: dict[TrustLevel, float] = {
        TrustLevel.low: 0.2,
        TrustLevel.medium: 0.5,
        TrustLevel.high: 0.9,
    }
    return mapping[t]


def numeric_to_trust(v: float | None | str) -> TrustLevel:
    if v is None:
        return TrustLevel.medium
    val = float(v)
    if val >= 0.7:
        return TrustLevel.high
    if val >= 0.35:
        return TrustLevel.medium
    return TrustLevel.low
