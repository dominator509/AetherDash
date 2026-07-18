"""Source configuration and canonical ingestion work items."""

from dataclasses import dataclass, field
from datetime import timedelta
from enum import IntEnum
from typing import Literal, Self

from pydantic import BaseModel, Field, field_validator, model_validator


class LadderRung(IntEnum):
    official_api = 1
    licensed_feed = 2
    rss_or_sitemap = 3
    robots_compliant_crawl = 4
    user_authorized_session = 5
    manual_review = 6


class SourceConfig(BaseModel):
    source: str = Field(min_length=1, max_length=255)
    rung: LadderRung
    interval_seconds: float = Field(ge=1.0, le=86_400.0)
    batch_size: int = Field(default=25, ge=1, le=500)
    enabled: bool = True
    requires_bot_bypass: bool = False

    @field_validator("requires_bot_bypass")
    @classmethod
    def reject_bot_bypass(cls, value: bool) -> bool:
        if value:
            raise ValueError(
                "anti-bot circumvention is prohibited by PROJECT_BRIEF and INV-4"
            )
        return value

    @property
    def interval(self) -> timedelta:
        return timedelta(seconds=self.interval_seconds)


class DowngradeDecision(BaseModel):
    """Explicit operator authorization to use a lower-compliance adapter."""

    source: str = Field(min_length=1, max_length=255)
    from_rung: LadderRung
    to_rung: LadderRung
    reason: str = Field(min_length=8, max_length=1_000)
    approved_by: str = Field(min_length=1, max_length=255)

    @model_validator(mode="after")
    def require_lower_compliance(self) -> Self:
        if self.to_rung <= self.from_rung:
            raise ValueError("downgrade target must be a lower-compliance rung")
        return self


class RuntimeSourceConfig(SourceConfig):
    adapter: Literal[
        "official_api",
        "licensed_feed",
        "rss_or_sitemap",
        "robots_compliant_crawl",
        "user_authorized_session",
        "manual_review",
    ]
    endpoint: str | None = None
    credential_headers: dict[str, str] = Field(default_factory=dict)
    paths: list[str] = Field(default_factory=list)
    user_agent: str | None = None
    min_interval_seconds: float | None = None
    downgrade: DowngradeDecision | None = None

    @model_validator(mode="after")
    def validate_adapter_settings(self) -> Self:
        expected = {
            "official_api": LadderRung.official_api,
            "licensed_feed": LadderRung.licensed_feed,
            "rss_or_sitemap": LadderRung.rss_or_sitemap,
            "robots_compliant_crawl": LadderRung.robots_compliant_crawl,
            "user_authorized_session": LadderRung.user_authorized_session,
            "manual_review": LadderRung.manual_review,
        }[self.adapter]
        actual = self.downgrade.to_rung if self.downgrade else self.rung
        if actual != expected:
            raise ValueError(
                "runtime adapter does not match the declared or downgraded rung"
            )
        if self.downgrade and (
            self.downgrade.source != self.source
            or self.downgrade.from_rung != self.rung
        ):
            raise ValueError(
                "runtime downgrade decision does not match its source config"
            )
        if self.adapter != "manual_review" and not self.endpoint:
            raise ValueError("network source adapter requires an endpoint")
        if (
            self.adapter in {"licensed_feed", "user_authorized_session"}
            and not self.credential_headers
        ):
            raise ValueError(
                "credentialed adapter requires header-to-environment mappings"
            )
        if self.adapter == "robots_compliant_crawl" and (
            not self.paths or not self.user_agent or self.min_interval_seconds is None
        ):
            raise ValueError("crawl adapter requires paths, user_agent, and rate limit")
        return self

    def scheduler_config(self) -> SourceConfig:
        return SourceConfig.model_validate(
            self.model_dump(
                include={
                    "source",
                    "rung",
                    "interval_seconds",
                    "batch_size",
                    "enabled",
                    "requires_bot_bypass",
                }
            )
        )


@dataclass(frozen=True)
class FetchedItem:
    kind: str
    content: str
    raw_content: bytes
    source: str
    trust: str = "medium"


@dataclass(frozen=True)
class FetchBatch:
    items: tuple[FetchedItem, ...] = field(default_factory=tuple)
    next_cursor: str | None = None


@dataclass
class SourceState:
    source: str
    rung: LadderRung
    cursor: str | None = None
    health: str = "unknown"
    consecutive_failures: int = 0
    last_error_code: str | None = None
    next_run_at: float = 0.0
