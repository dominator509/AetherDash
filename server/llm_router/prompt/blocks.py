"""Static block registry for cache-first prompt assembly.

Each block is a fixed string that forms part of the cacheable prompt prefix.
Changing a block changes the cache key and invalidates cached prefixes.
"""

STATIC_BLOCKS: dict[str, str] = {
    "system": (
        "You are AETHER, an AI-native trading terminal. You operate across "
        "prediction markets, equities/options, crypto/DeFi, and sports-event "
        "markets. Your responses are concise, data-driven, and grounded in "
        "the provided context. Never speculate beyond the reference data.\n"
        "\n"
        "Core principles:\n"
        "- Prioritise deterministic execution paths over AI judgement.\n"
        "- Risk decisions are computed by the risk engine, not inferred.\n"
        "- Wallet operations require human approval via the Guardian.\n"
        "- All trades are logged to the audit trail.\n"
    ),
    "tools": (
        "Available tool categories:\n"
        "- Market data: query prices, order books, funding rates\n"
        "- Trading: place, cancel, amend orders\n"
        "- Risk: check limits, margin, exposure\n"
        "- Wallet: view balances, propose withdrawals (requires approval)\n"
        "- Analytics: P&L, performance, historical snapshots\n"
    ),
    "ontology": (
        "Domain ontology:\n"
        "- venue: a specific exchange or platform (e.g. Polymarket, Binance)\n"
        "- instrument: a tradeable asset or contract\n"
        "- order: a request to trade (limit, market, stop)\n"
        "- position: an open exposure to an instrument\n"
        "- wallet: a custodial or self-custodied account\n"
        "- guardian: the human-in-the-loop approval service\n"
    ),
    "summarize_instruction": (
        "Summarize the following text in <=500 chars plain language. "
        "Focus on actionable information. Omit boilerplate. "
        "Use compact notation for numbers ($1.2M, 4.5K)."
    ),
    "extract_instruction": (
        "Extract entities, dates, and claims from the following text. "
        "Return a structured JSON object with keys: entities (list), "
        "dates (list of ISO-8601), claims (list of strings). "
        "If a field has no values, return an empty list."
    ),
}


def get_static_blocks(
    static_context_ref: str | None = None,
) -> dict[str, str]:
    """Return the static block set, optionally filtered by context ref.

    Args:
        static_context_ref: Optional key to select a specific block set.
            Currently unused — returns the default ``STATIC_BLOCKS``.

    Returns:
        A copy of the relevant ``STATIC_BLOCKS`` dict.
    """
    return dict(STATIC_BLOCKS)
