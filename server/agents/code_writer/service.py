from __future__ import annotations

import asyncio
import json
import os
from collections.abc import Awaitable, Callable
from typing import Any, Protocol

from pydantic import ValidationError

from server.llm_router.client import complete

from .models import GeneratedPluginDraft, PluginGenerationRequest

Completion = Callable[..., Awaitable[dict[str, Any]]]


class GenerationError(ValueError):
    pass


class PluginSubmitter(Protocol):
    async def submit(self, draft: GeneratedPluginDraft) -> dict[str, str]: ...


class RustPluginSubmitter:
    """Shell-free stdin boundary to the EP-403 compiler/installer binary."""

    def __init__(self, executable: str | None = None, timeout: float = 30.0) -> None:
        self._executable = executable or os.environ.get(
            "AETHER_PLUGIN_SUBMIT_BIN", "aether-plugin-submit"
        )
        self._timeout = timeout

    async def submit(self, draft: GeneratedPluginDraft) -> dict[str, str]:
        try:
            process = await asyncio.create_subprocess_exec(
                self._executable,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.DEVNULL,
            )
        except OSError as exc:
            raise GenerationError("plugin submitter is unavailable") from exc
        try:
            stdout, _ = await asyncio.wait_for(
                process.communicate(draft.model_dump_json().encode()), self._timeout
            )
        except TimeoutError as exc:
            process.kill()
            await process.wait()
            raise GenerationError("plugin submission timed out") from exc
        if process.returncode != 0:
            raise GenerationError("EP-403 rejected plugin submission")
        try:
            result = json.loads(stdout)
        except json.JSONDecodeError as exc:
            raise GenerationError("plugin submitter returned invalid evidence") from exc
        expected = {"name": draft.name, "version": draft.version, "status": "installed"}
        if result != expected:
            raise GenerationError("plugin submitter returned mismatched evidence")
        return expected


class CodeWriter:
    def __init__(self, completion: Completion = complete) -> None:
        self._completion = completion

    async def generate(self, request: PluginGenerationRequest) -> GeneratedPluginDraft:
        response = await self._completion(
            "code_plugin",
            dynamic_inputs={
                "user_text": json.dumps(
                    {
                        "objective": request.objective,
                        "allowed_capabilities": sorted(request.allowed_capabilities),
                    },
                    sort_keys=True,
                )
            },
            model_policy=request.model_policy,
            static_context_ref="code_plugin",
            max_tokens=8_192,
        )
        if response.get("error"):
            raise GenerationError("plugin generation provider failed")
        text = response.get("text")
        if not isinstance(text, str):
            raise GenerationError("plugin generation returned no JSON text")
        try:
            raw = json.loads(text)
            draft = GeneratedPluginDraft.model_validate(raw)
        except (json.JSONDecodeError, ValidationError) as exc:
            raise GenerationError(
                "plugin generation returned an invalid draft"
            ) from exc
        requested = set(draft.capabilities)
        if not requested.issubset(request.allowed_capabilities):
            raise GenerationError("generated plugin requested an unapproved capability")
        if "network_http" not in requested and draft.network_allowlist:
            raise GenerationError("network allowlist requires network_http capability")
        if "network_http" in requested and not draft.network_allowlist:
            raise GenerationError("network_http requires an exact hostname allowlist")
        return draft

    async def generate_and_submit(
        self, request: PluginGenerationRequest, submitter: PluginSubmitter
    ) -> dict[str, str]:
        return await submitter.submit(await self.generate(request))
