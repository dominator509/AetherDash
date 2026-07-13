"""Provider configuration and routing table for the LLM Router.

Routing maps: purpose -> model_policy -> (provider, model)
"""

import os

# ---------------------------------------------------------------------------
# Routing table
# ---------------------------------------------------------------------------

ROUTING: dict[str, dict[str, tuple[str, str]]] = {
    "summarize": {
        "default": ("anthropic", "claude-haiku-4-5-20251001"),
        "cheap": ("deepseek", "deepseek-chat"),
        "local": ("ollama", "llama3.2:3b"),
        "xai": ("xai", "grok-4.5"),
    },
    "extract": {
        "default": ("anthropic", "claude-haiku-4-5-20251001"),
        "cheap": ("deepseek", "deepseek-chat"),
        "local": ("ollama", "llama3.2:3b"),
        "xai": ("xai", "grok-4.5"),
    },
    "embed": {
        "default": ("openai", "text-embedding-3-small"),
        "cheap": ("ollama", "nomic-embed-text"),
        "local": ("ollama", "nomic-embed-text"),
    },
    "chat": {
        "default": ("anthropic", "claude-sonnet-5"),
        "cheap": ("deepseek", "deepseek-chat"),
        "local": ("ollama", "llama3.2:3b"),
        "xai": ("xai", "grok-4.5"),
    },
}

# ---------------------------------------------------------------------------
# Provider API keys (read from environment)
# ---------------------------------------------------------------------------

# Each provider key is optional — if missing, the router will report a clear
# error rather than crash at import time.

_SUPPORTED_PURPOSES: set[str] = set(ROUTING.keys())
_SUPPORTED_POLICIES: set[str] = {"default", "cheap", "local", "xai"}


def get_api_keys() -> dict[str, str | None]:
    """Return a dict of provider -> API key (None if unset)."""
    return {
        "anthropic": os.environ.get("AETHER_LLM__ANTHROPIC_API_KEY"),
        "deepseek": os.environ.get("AETHER_LLM__DEEPSEEK_API_KEY"),
        "openai": os.environ.get("AETHER_LLM__OPENAI_API_KEY"),
        "xai": os.environ.get("AETHER_LLM__XAI_API_KEY"),
    }


def get_ollama_base() -> str:
    """Return the Ollama base URL from env or default."""
    return os.environ.get("AETHER_LLM__LOCAL_ENDPOINT", "http://localhost:11434")


def validate_purpose(purpose: str) -> None:
    """Raise ValueError for unknown purposes."""
    if purpose not in _SUPPORTED_PURPOSES:
        raise ValueError(
            f"Unknown purpose {purpose!r}. "
            f"Supported purposes: {sorted(_SUPPORTED_PURPOSES)}"
        )


def validate_policy(policy: str) -> None:
    """Raise ValueError for unknown model policies."""
    if policy not in _SUPPORTED_POLICIES:
        raise ValueError(
            f"Unknown model_policy {policy!r}. "
            f"Supported policies: {sorted(_SUPPORTED_POLICIES)}"
        )


def lookup(purpose: str, model_policy: str = "default") -> tuple[str, str]:
    """Look up (provider, model) for a given purpose and model_policy.

    Raises ValueError if the purpose or policy is unknown.
    """
    validate_purpose(purpose)
    validate_policy(model_policy)
    if model_policy not in ROUTING[purpose]:
        raise ValueError(
            f"model_policy {model_policy!r} is not supported for purpose {purpose!r}"
        )
    return ROUTING[purpose][model_policy]
