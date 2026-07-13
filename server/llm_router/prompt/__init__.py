"""Cache-first prompt builder for the AETHER LLM Router."""

from server.llm_router.prompt.blocks import STATIC_BLOCKS
from server.llm_router.prompt.builder import PromptAssembly, build_prompt

__all__ = [
    "STATIC_BLOCKS",
    "PromptAssembly",
    "build_prompt",
]
