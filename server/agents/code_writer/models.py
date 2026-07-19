from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator

CapabilityName = Literal[
    "read_markets",
    "read_positions",
    "submit_alerts",
    "access_brain",
    "execute_paper",
    "network_http",
]
PluginKindName = Literal["scanner", "strategy", "alert", "dashboard", "data_import"]


class PluginDependencyDraft(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9_-]+$")
    version: str = Field(min_length=1, max_length=64)


class PluginGenerationRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    objective: str = Field(min_length=1, max_length=4_000)
    allowed_capabilities: set[CapabilityName] = Field(min_length=1)
    model_policy: Literal["default", "cheap", "local"] = "default"


class GeneratedPluginDraft(BaseModel):
    """Unsigned and inert output. Only the Rust EP-403 gate may make it loadable."""

    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9_-]+$")
    version: str = Field(min_length=1, max_length=64)
    description: str = Field(min_length=1, max_length=1_000)
    author: str = Field(min_length=1, max_length=256)
    kind: PluginKindName
    capabilities: list[CapabilityName] = Field(min_length=1)
    network_allowlist: list[str] = Field(default_factory=list, max_length=32)
    dependencies: list[PluginDependencyDraft] = Field(
        default_factory=list, max_length=256
    )
    entry_point: str = Field(default="run", pattern=r"^[A-Za-z_][A-Za-z0-9_]*$")
    config_schema: dict[str, str] = Field(default_factory=dict)
    wat_source: str = Field(min_length=1, max_length=262_144)

    @field_validator("capabilities")
    @classmethod
    def capabilities_are_unique(
        cls, values: list[CapabilityName]
    ) -> list[CapabilityName]:
        if len(values) != len(set(values)):
            raise ValueError("capabilities must be unique")
        return values

    @field_validator("network_allowlist")
    @classmethod
    def hosts_are_exact(cls, values: list[str]) -> list[str]:
        for host in values:
            if not host or len(host) > 253 or any(char in host for char in "/:@*"):
                raise ValueError("network allowlist entries must be exact hostnames")
        return values
