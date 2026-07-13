"""Unit tests for the cache-first prompt builder.

Tests cover INV-3 prefix stability, block ordering, cache-key determinism,
breakpoint integrity, RAG compaction, empty-input resilience, and
purpose-specific instruction selection.
"""

from __future__ import annotations

import pytest

from server.llm_router.prompt import STATIC_BLOCKS
from server.llm_router.prompt.builder import build_prompt

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _find_index(messages: list[dict[str, str]], substring: str) -> int | None:
    """Return the index of the first message containing *substring*."""
    for i, m in enumerate(messages):
        if substring in m["content"]:
            return i
    return None


# ---------------------------------------------------------------------------
# INV-3 prefix stability
# ---------------------------------------------------------------------------


class TestPrefixStability:
    """INV-3: same static blocks -> identical prefix bytes."""

    @pytest.mark.asyncio
    async def test_prefix_stability(self):
        """Same static blocks + same dynamic inputs -> identical
        static_prefix_bytes."""
        a1 = await build_prompt(
            "summarize", dynamic_inputs={"query": "What is the P&L?"}
        )
        a2 = await build_prompt(
            "summarize", dynamic_inputs={"query": "What is the P&L?"}
        )
        assert a1.static_prefix_bytes == a2.static_prefix_bytes

    @pytest.mark.asyncio
    async def test_different_dynamic_same_prefix(self):
        """Different dynamic inputs, same static blocks -> same
        static_prefix_bytes but different cache_key."""
        a1 = await build_prompt(
            "summarize", dynamic_inputs={"query": "Show me BTC dominance"}
        )
        a2 = await build_prompt(
            "summarize", dynamic_inputs={"query": "What is ETH gas?"}
        )
        # INV-3: static prefix is unchanged
        assert a1.static_prefix_bytes == a2.static_prefix_bytes
        # Cache keys differ because the full prompt differs
        assert a1.cache_key != a2.cache_key

    @pytest.mark.asyncio
    async def test_different_purpose_different_prefix(self):
        """Different purposes produce different static prefixes
        (different instruction block)."""
        summ = await build_prompt("summarize")
        ext = await build_prompt("extract")
        assert summ.static_prefix_bytes != ext.static_prefix_bytes


# ---------------------------------------------------------------------------
# Block ordering
# ---------------------------------------------------------------------------


class TestBlockOrdering:
    """Static blocks always precede dynamic data."""

    @pytest.mark.asyncio
    async def test_block_ordering(self):
        """Static blocks always come before dynamic messages."""
        assembly = await build_prompt(
            "summarize",
            dynamic_inputs={"query": "check"},
            rag_chunks=["doc about prices", "doc about volumes"],
        )

        # All messages before breakpoint_index are static (role=system)
        for i in range(assembly.breakpoint_index):
            assert assembly.messages[i]["role"] == "system"

        # The breakpoint marker itself is a system message
        bp_idx = assembly.breakpoint_index - 1  # last static message
        assert "CACHE BREAKPOINT" in assembly.messages[bp_idx]["content"]

        # Dynamic messages come after the breakpoint
        assert assembly.breakpoint_index < len(assembly.messages)

    @pytest.mark.asyncio
    async def test_dynamic_data_after_breakpoint(self):
        """No dynamic data appears before the breakpoint index."""
        assembly = await build_prompt(
            "summarize",
            dynamic_inputs={"user_text": "Analyze ETH/BTC"},
            rag_chunks=["market data chunk", "news chunk"],
        )

        # RAG references and user text are NOT in static messages
        for i in range(assembly.breakpoint_index):
            content = assembly.messages[i]["content"]
            assert "[REF-" not in content, f"static message {i} has RAG ref"
            assert "Analyze ETH/BTC" not in content, f"static message {i} has user text"

        # They ARE in dynamic messages
        found_rag = False
        found_user = False
        for i in range(assembly.breakpoint_index, len(assembly.messages)):
            content = assembly.messages[i]["content"]
            if "[REF-1]" in content:
                found_rag = True
            if "Analyze ETH/BTC" in content:
                found_user = True
        assert found_rag, "RAG ref not found in dynamic messages"
        assert found_user, "User text not found in dynamic messages"


# ---------------------------------------------------------------------------
# Cache key determinism
# ---------------------------------------------------------------------------


class TestCacheKey:
    """Cache key is deterministic for identical inputs."""

    @pytest.mark.asyncio
    async def test_cache_key_deterministic(self):
        """Same static context -> same cache_key."""
        a1 = await build_prompt("summarize", static_context_ref="default")
        a2 = await build_prompt("summarize", static_context_ref="default")
        assert a1.cache_key == a2.cache_key

    @pytest.mark.asyncio
    async def test_cache_key_with_dynamic(self):
        """Same dynamic inputs -> same cache_key."""
        kwargs = dict(
            purpose="extract",
            dynamic_inputs={"query": "Parse this report"},
            rag_chunks=["financial data"],
        )
        a1 = await build_prompt(**kwargs)
        a2 = await build_prompt(**kwargs)
        assert a1.cache_key == a2.cache_key


# ---------------------------------------------------------------------------
# RAG chunk compact IDs
# ---------------------------------------------------------------------------


class TestRagChunks:
    """RAG chunks are injected as compact ID references."""

    @pytest.mark.asyncio
    async def test_rag_chunks_as_compact_ids(self):
        """RAG chunks appear as [REF-N] references in the dynamic tail."""
        chunks = [
            "BTC sitting at $67,200 with 24h volume of $28B",
            "ETH/BTC ratio dropped to 0.042, lowest since March",
        ]
        assembly = await build_prompt(
            "summarize",
            rag_chunks=chunks,
        )

        # Find the RAG message (should be first dynamic message)
        assert assembly.breakpoint_index < len(assembly.messages)
        rag_msg = assembly.messages[assembly.breakpoint_index]
        assert "[REF-1]" in rag_msg["content"]
        assert "[REF-2]" in rag_msg["content"]

        # Actual chunk text is included alongside the compact ref
        assert "BTC sitting" in rag_msg["content"]
        assert "ETH/BTC ratio" in rag_msg["content"]

    @pytest.mark.asyncio
    async def test_no_rag_chunks_omits_rag_message(self):
        """When no RAG chunks are given, no RAG message is added."""
        assembly = await build_prompt("summarize")
        for msg in assembly.messages:
            assert "[REF-" not in msg["content"]

    @pytest.mark.asyncio
    async def test_rag_label_prefix(self):
        """The RAG message starts with a 'Reference documents:' label."""
        assembly = await build_prompt(
            "summarize",
            rag_chunks=["just one chunk"],
        )
        assert assembly.breakpoint_index < len(assembly.messages)
        content = assembly.messages[assembly.breakpoint_index]["content"]
        assert content.startswith("Reference documents:")


# ---------------------------------------------------------------------------
# Empty / edge cases
# ---------------------------------------------------------------------------


class TestEmptyDynamic:
    """Prompt works correctly with no dynamic inputs."""

    @pytest.mark.asyncio
    async def test_empty_dynamic(self):
        """Building a prompt with no dynamic inputs succeeds."""
        assembly = await build_prompt("summarize")
        assert len(assembly.messages) > 0
        assert len(assembly.static_prefix_bytes) > 0
        assert assembly.cache_key != ""
        # All messages are static (no dynamic tail)
        assert assembly.breakpoint_index == len(assembly.messages)

    @pytest.mark.asyncio
    async def test_empty_inputs_vs_none(self):
        """Empty dict and None for dynamic_inputs produce the same prompt."""
        a1 = await build_prompt("summarize", dynamic_inputs=None)
        a2 = await build_prompt("summarize", dynamic_inputs={})
        assert a1.static_prefix_bytes == a2.static_prefix_bytes
        assert a1.cache_key == a2.cache_key


# ---------------------------------------------------------------------------
# Purpose-specific instructions
# ---------------------------------------------------------------------------


class TestPurposeInstructions:
    """Different purposes select different instruction blocks."""

    @pytest.mark.asyncio
    async def test_summarize_includes_summarize_block(self):
        """The summarize purpose includes the summarize_instruction block."""
        assembly = await build_prompt("summarize")
        all_content = "".join(m["content"] for m in assembly.messages)
        assert "Summarize the following text" in all_content
        assert STATIC_BLOCKS["summarize_instruction"] in all_content

    @pytest.mark.asyncio
    async def test_extract_includes_extract_block(self):
        """The extract purpose includes the extract_instruction block."""
        assembly = await build_prompt("extract")
        all_content = "".join(m["content"] for m in assembly.messages)
        assert "Extract entities" in all_content
        assert STATIC_BLOCKS["extract_instruction"] in all_content

    @pytest.mark.asyncio
    async def test_purpose_specific_instructions_differ(self):
        """summarize vs extract get different static instructions."""
        summ = await build_prompt("summarize")
        ext = await build_prompt("extract")

        summ_content = "".join(m["content"] for m in summ.messages)
        ext_content = "".join(m["content"] for m in ext.messages)

        assert "Summarize the following" in summ_content
        assert "Extract entities" in ext_content

    @pytest.mark.asyncio
    async def test_chat_has_no_extra_instruction(self):
        """The 'chat' purpose has no special instruction block."""
        assembly = await build_prompt("chat")
        all_content = "".join(m["content"] for m in assembly.messages)
        assert "Summarize the following" not in all_content
        assert "Extract entities" not in all_content


# ---------------------------------------------------------------------------
# PromptAssembly shape
# ---------------------------------------------------------------------------


class TestPromptAssemblyShape:
    """PromptAssembly dataclass shape checks."""

    @pytest.mark.asyncio
    async def test_assembly_dataclass_fields(self):
        """PromptAssembly has the expected fields."""
        assembly = await build_prompt("summarize")

        assert hasattr(assembly, "messages")
        assert hasattr(assembly, "cache_key")
        assert hasattr(assembly, "static_prefix_bytes")
        assert hasattr(assembly, "breakpoint_index")

        assert isinstance(assembly.messages, list)
        assert isinstance(assembly.cache_key, str)
        assert isinstance(assembly.static_prefix_bytes, bytes)
        assert isinstance(assembly.breakpoint_index, int)

    @pytest.mark.asyncio
    async def test_messages_are_openai_style(self):
        """Each message has at least 'role' and 'content' keys."""
        assembly = await build_prompt(
            "extract",
            dynamic_inputs={"query": "test"},
            rag_chunks=["chunk"],
        )
        for msg in assembly.messages:
            assert "role" in msg
            assert "content" in msg
            assert msg["role"] in ("system", "user", "assistant")

    @pytest.mark.asyncio
    async def test_breakpoint_index_type(self):
        """breakpoint_index is a valid list index."""
        assembly = await build_prompt("summarize")
        assert 0 <= assembly.breakpoint_index <= len(assembly.messages)


# ---------------------------------------------------------------------------
# Static blocks registry shape
# ---------------------------------------------------------------------------


class TestStaticBlocks:
    """STATIC_BLOCKS has the expected keys."""

    def test_has_required_keys(self):
        expected = {
            "system",
            "tools",
            "ontology",
            "summarize_instruction",
            "extract_instruction",
        }
        assert expected.issubset(STATIC_BLOCKS.keys())

    def test_all_values_are_strings(self):
        for key, val in STATIC_BLOCKS.items():
            assert isinstance(val, str), f"Block {key!r} is not a string"
            assert len(val) > 0, f"Block {key!r} is empty"
