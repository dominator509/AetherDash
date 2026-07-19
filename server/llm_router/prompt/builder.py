"""Cache-first prompt builder for the AETHER LLM Router.

Assembles prompts with a cacheable static prefix (system, tools, ontology,
examples) followed by a single cache breakpoint and per-call dynamic data.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from typing import Any

from server.llm_router.prompt.blocks import STATIC_BLOCKS

# ---------------------------------------------------------------------------
# Cache breakpoint marker
# ---------------------------------------------------------------------------

_CACHE_BREAKPOINT = "--- CACHE BREAKPOINT ---"
"""Single breakpoint inserted before per-call dynamic data."""

# ---------------------------------------------------------------------------
# PromptAssembly type
# ---------------------------------------------------------------------------


@dataclass
class PromptAssembly:
    """A fully assembled prompt with cache-first prefix stability.

    Attributes:
        messages: OpenAI-style message list.
        cache_key: SHA-256 of the full assembled message content for
            response-cache lookup.
        static_prefix_bytes: Bytes of the static prefix (before the
            breakpoint) for INV-3 prefix-stability assertions.
        breakpoint_index: Index of the first dynamic message.
    """

    messages: list[dict[str, str]] = field(default_factory=list)
    cache_key: str = ""
    static_prefix_bytes: bytes = b""
    breakpoint_index: int = 0


# ---------------------------------------------------------------------------
# Purpose -> instruction block mapping
# ---------------------------------------------------------------------------

_PURPOSE_INSTRUCTION: dict[str, str] = {
    "summarize": "summarize_instruction",
    "extract": "extract_instruction",
    "code_plugin": "code_plugin_instruction",
}


def _get_purpose_instruction_key(purpose: str) -> str | None:
    """Return the ``STATIC_BLOCKS`` key for a given purpose, or ``None``."""
    return _PURPOSE_INSTRUCTION.get(purpose)


def _build_static_messages(purpose: str) -> list[dict[str, str]]:
    """Build the static prefix messages (before the cache breakpoint).

    Order: system -> tools -> ontology -> purpose instruction -> breakpoint.
    """
    messages: list[dict[str, str]] = []

    # Always-included static blocks in fixed order
    for key in ("system", "tools", "ontology"):
        content = STATIC_BLOCKS.get(key)
        if content:
            messages.append({"role": "system", "content": content})

    # Purpose-specific instruction block
    instr_key = _get_purpose_instruction_key(purpose)
    if instr_key and instr_key in STATIC_BLOCKS:
        messages.append({"role": "system", "content": STATIC_BLOCKS[instr_key]})

    # Cache breakpoint marker — still part of the static prefix
    messages.append({"role": "system", "content": _CACHE_BREAKPOINT})

    return messages


def _build_dynamic_messages(
    dynamic_inputs: dict[str, Any] | None,
    rag_chunks: list[str] | None,
) -> list[dict[str, str]]:
    """Build the dynamic tail messages (after the cache breakpoint)."""
    messages: list[dict[str, str]] = []

    # RAG chunks as compact ID references
    if rag_chunks:
        rag_lines: list[str] = ["Reference documents:"]
        for i, chunk in enumerate(rag_chunks, start=1):
            rag_lines.append(f"[REF-{i}] {chunk}")
        messages.append({"role": "system", "content": "\n".join(rag_lines)})

    # Per-call user input
    if dynamic_inputs:
        user_text = dynamic_inputs.get("user_text") or dynamic_inputs.get("query")
        if user_text:
            messages.append({"role": "user", "content": str(user_text)})

    return messages


def _join_content_bytes(messages: list[dict[str, str]]) -> bytes:
    """Join message content fields into a single byte string for hashing."""
    return b"".join(m["content"].encode("utf-8") for m in messages)


async def build_prompt(
    purpose: str,
    static_context_ref: str | None = None,
    dynamic_inputs: dict[str, Any] | None = None,
    rag_chunks: list[str] | None = None,
) -> PromptAssembly:
    """Build a prompt with cache-first prefix stability.

    Steps
        1. Static blocks (system, tools, ontology, purpose instruction)
           are placed first — ALWAYS.
        2. A single cache breakpoint marker is inserted.
        3. The dynamic tail (user query / RAG chunk compact IDs) follows.

    Args:
        purpose: One of ``"summarize"``, ``"extract"``, ``"embed"``, ``"chat"``.
        static_context_ref: Key into the static block registry
            (reserved for future use).
        dynamic_inputs: Per-call dynamic data
            (keys: ``query``, ``user_text``, ``filters``, etc.).
        rag_chunks: Pre-retrieved RAG chunks injected as compact ID references.

    Returns:
        A ``PromptAssembly`` with cache-first structure.
    """
    # 1. Static blocks — always first (includes breakpoint marker)
    static_messages = _build_static_messages(purpose)
    breakpoint_index = len(static_messages)

    # 2. Dynamic tail
    dynamic_messages = _build_dynamic_messages(dynamic_inputs, rag_chunks)

    # 3. Full message list
    all_messages = static_messages + dynamic_messages

    # 4. INV-3: static prefix bytes (for prefix-stability assertions)
    static_prefix_bytes = _join_content_bytes(static_messages)

    # 5. Cache key — SHA-256 of the full prompt content
    full_content_bytes = _join_content_bytes(all_messages)
    cache_key = hashlib.sha256(full_content_bytes).hexdigest()

    return PromptAssembly(
        messages=all_messages,
        cache_key=cache_key,
        static_prefix_bytes=static_prefix_bytes,
        breakpoint_index=breakpoint_index,
    )
