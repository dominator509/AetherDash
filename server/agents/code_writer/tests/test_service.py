from __future__ import annotations

import json

import pytest

from server.agents.code_writer import (
    CodeWriter,
    GeneratedPluginDraft,
    GenerationError,
    PluginGenerationRequest,
    RustPluginSubmitter,
)


def draft(capabilities: list[str]) -> dict[str, object]:
    return {
        "name": "generated-market-reader",
        "version": "1.0.0",
        "description": "Reads market data",
        "author": "aether-code-writer",
        "kind": "strategy",
        "capabilities": capabilities,
        "network_allowlist": [],
        "dependencies": [],
        "entry_point": "run",
        "config_schema": {},
        "wat_source": '(module (import "aether" "read_markets" (func $read (result i32))) (func (export "run") (result i32) call $read))',
    }


@pytest.mark.asyncio
async def test_generated_draft_is_cache_first_and_remains_unsigned() -> None:
    calls: list[tuple[tuple[object, ...], dict[str, object]]] = []

    async def completion(*args: object, **kwargs: object) -> dict[str, object]:
        calls.append((args, kwargs))
        return {"text": json.dumps(draft(["read_markets"]))}

    result = await CodeWriter(completion).generate(
        PluginGenerationRequest(
            objective="read markets", allowed_capabilities={"read_markets"}
        )
    )
    assert result.name == "generated-market-reader"
    assert "signature" not in result.model_dump()
    assert calls[0][0] == ("code_plugin",)
    assert calls[0][1]["static_context_ref"] == "code_plugin"


@pytest.mark.asyncio
async def test_generation_cannot_expand_capabilities_or_smuggle_fields() -> None:
    async def over_scoped(*_args: object, **_kwargs: object) -> dict[str, object]:
        return {"text": json.dumps(draft(["read_markets", "execute_paper"]))}

    request = PluginGenerationRequest(
        objective="read markets", allowed_capabilities={"read_markets"}
    )
    with pytest.raises(GenerationError, match="unapproved capability"):
        await CodeWriter(over_scoped).generate(request)

    payload = draft(["read_markets"])
    payload["signature"] = "forged"

    async def smuggled(*_args: object, **_kwargs: object) -> dict[str, object]:
        return {"text": json.dumps(payload)}

    with pytest.raises(GenerationError, match="invalid draft"):
        await CodeWriter(smuggled).generate(request)


@pytest.mark.asyncio
async def test_generation_submission_can_only_create_installed_evidence() -> None:
    async def completion(*_args: object, **_kwargs: object) -> dict[str, object]:
        return {"text": json.dumps(draft(["read_markets"]))}

    class RecordingSubmitter:
        async def submit(self, generated: object) -> dict[str, str]:
            assert generated.name == "generated-market-reader"
            return {
                "name": "generated-market-reader",
                "version": "1.0.0",
                "status": "installed",
            }

    result = await CodeWriter(completion).generate_and_submit(
        PluginGenerationRequest(
            objective="read markets", allowed_capabilities={"read_markets"}
        ),
        RecordingSubmitter(),
    )
    assert result["status"] == "installed"
    assert "approved" not in result.values()


@pytest.mark.asyncio
async def test_missing_rust_submitter_fails_at_the_generation_boundary(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    async def missing(*_args: object, **_kwargs: object) -> object:
        raise FileNotFoundError("not installed")

    monkeypatch.setattr("asyncio.create_subprocess_exec", missing)
    generated = GeneratedPluginDraft.model_validate(draft(["read_markets"]))

    with pytest.raises(GenerationError, match="submitter is unavailable"):
        await RustPluginSubmitter("missing-aether-plugin-submit").submit(generated)
