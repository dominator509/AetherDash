"""Obsidian-compatible Markdown vault generator.

SPEC-011 contract:
    - Vault is generated, one-way: DB -> Markdown.
    - Folders by kind and by market (``kind/market_key/`` or ``kind/_no_market/``).
    - Unique filenames: ``{object_id}.md``.
    - Wikilinks from linked_events/market_keys.
    - YAML frontmatter: object ID + provenance hash (properly escaped).
    - EXCLUDED: raw email bodies, low-trust inbox content beyond summaries.
    - ``vault/**`` gitignored except ``.gitkeep``.
    - CI treats vault diff as failure.
"""

import logging
import shutil
from pathlib import Path

from server.brain import store as brain_store
from server.brain.models import BrainObject, TrustLevel

logger = logging.getLogger(__name__)

# Fixed vault directory (no arbitrary path parameter per INV-9).
_VAULT_DIR = "vault"

# ---------------------------------------------------------------------------
# Exclusion helpers
# ---------------------------------------------------------------------------

_EXCLUDED_KINDS_FOR_RAW: frozenset[str] = frozenset({"email"})


def _should_include_object(obj: BrainObject) -> bool:
    """Determine whether a BrainObject should be included in the vault.

    Exclusion rules:
    - Email objects: only include the summary, not the raw body.
    - Low-trust (low) inbox objects: only include if they have a summary.
    - Cold-tier objects ARE included (they have summaries and metadata).
    """
    trust_str = obj.trust.value if isinstance(obj.trust, TrustLevel) else str(obj.trust)
    origin_str = obj.origin.value if hasattr(obj.origin, "value") else str(obj.origin)

    # Low-trust inbox: only include if summarised
    if trust_str == "low" and origin_str == "inbox":
        if not obj.summary:
            return False

    return True


def _should_include_raw_body(obj: BrainObject) -> bool:
    """Determine whether the vault page should include the full raw body.

    Exclusion rules:
    - Email kind: NEVER include raw body (only summary).
    """
    kind_str = obj.kind.value if hasattr(obj.kind, "value") else str(obj.kind)
    return kind_str not in _EXCLUDED_KINDS_FOR_RAW


# ---------------------------------------------------------------------------
# YAML frontmatter (manual quoting -- avoids yaml dependency)
# ---------------------------------------------------------------------------


def _yaml_scalar(value: object) -> str:
    """Format a Python value as a safe YAML scalar.

    Strings are double-quoted if they contain characters that could break
    YAML parsing (colons, hashes, brackets, quotes, etc.).  Numeric values
    and None are formatted directly.
    """
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int | float):
        return str(value)
    s = str(value)
    # Double-quote if the string could confuse a YAML parser
    if not s or any(ch in s for ch in ":#{}[]&*!|>%'\"`@,?"):
        escaped = s.replace("\\", "\\\\").replace('"', '\\"')
        return f'"{escaped}"'
    return s


def _yaml_frontmatter(obj: BrainObject) -> str:
    """Generate YAML frontmatter for a BrainObject.

    Fields: id, kind, provenance_hash, source, trust, origin,
    author_or_publisher, published_ts, ingested_ts, tier, confidence.
    """
    lines = ["---"]
    lines.append(f"id: {_yaml_scalar(obj.id)}")
    lines.append(
        f"kind: {_yaml_scalar(obj.kind.value if hasattr(obj.kind, 'value') else obj.kind)}"
    )
    lines.append(f"provenance_hash: {_yaml_scalar(obj.provenance_hash)}")
    lines.append(f"source: {_yaml_scalar(obj.source)}")
    lines.append(
        f"trust: {_yaml_scalar(obj.trust.value if isinstance(obj.trust, TrustLevel) else obj.trust)}"
    )
    lines.append(
        f"origin: {_yaml_scalar(obj.origin.value if hasattr(obj.origin, 'value') else obj.origin)}"
    )
    if obj.author_or_publisher:
        lines.append(f"author_or_publisher: {_yaml_scalar(obj.author_or_publisher)}")
    if obj.published_ts:
        lines.append(f"published_ts: {_yaml_scalar(obj.published_ts)}")
    lines.append(f"ingested_ts: {_yaml_scalar(obj.ingested_ts)}")
    lines.append(
        f"tier: {_yaml_scalar(obj.tier.value if hasattr(obj.tier, 'value') else obj.tier)}"
    )
    if obj.confidence is not None:
        lines.append(f"confidence: {_yaml_scalar(obj.confidence)}")
    lines.append("---")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Markdown body helpers
# ---------------------------------------------------------------------------


def _wikilinks(values: list[str]) -> str:
    """Format a list of values as Obsidian wikilinks.

    Each value becomes ``[[value]]`` on its own line.
    Returns empty string if the list is empty.
    """
    if not values:
        return ""
    return "\n".join(f"- [[{v}]]" for v in values)


def _body_content(obj: BrainObject) -> str:
    """Generate the body content for a BrainObject vault page.

    Calls ``_should_include_raw_body`` to gate the raw body section.
    Includes:
    - Summary (mandatory, first section)
    - Linked events as wikilinks
    - Market keys as wikilinks
    - Entities list
    - Raw/clean refs
    """
    parts: list[str] = []

    # Title
    title = obj.summary or f"{obj.kind.value}: {obj.id}"
    parts.append(f"# {title}")
    parts.append("")

    # Summary section
    if obj.summary:
        parts.append("## Summary")
        parts.append("")
        parts.append(obj.summary)
        parts.append("")

    # Raw body section (gated by exclusion helper)
    if _should_include_raw_body(obj):
        parts.append("## Raw Body")
        parts.append("")
        parts.append(f"See MinIO ref: `{obj.raw_ref}`")
        parts.append("")

    # Linked events as wikilinks
    if obj.linked_events:
        parts.append("## Linked Events")
        parts.append("")
        parts.extend(_wikilinks(obj.linked_events).split("\n"))
        parts.append("")

    # Market keys as wikilinks
    if obj.market_keys:
        parts.append("## Market Keys")
        parts.append("")
        parts.extend(_wikilinks(obj.market_keys).split("\n"))
        parts.append("")

    # Entities
    if obj.entities:
        parts.append("## Entities")
        parts.append("")
        for ent in obj.entities:
            parts.append(f"- {ent}")
        parts.append("")

    # Provenance metadata
    parts.append("## Provenance")
    parts.append("")
    parts.append(f"- **Provenance Hash:** `{obj.provenance_hash}`")
    if obj.raw_ref:
        parts.append(f"- **Raw Ref:** `{obj.raw_ref}`")
    if obj.clean_ref:
        parts.append(f"- **Clean Ref:** `{obj.clean_ref}`")
    if obj.url_or_ref:
        parts.append(f"- **URL:** {obj.url_or_ref}")
    if obj.source:
        parts.append(f"- **Source:** {obj.source}")

    return "\n".join(parts)


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


async def regenerate_vault() -> None:
    """Generate Obsidian-compatible Markdown vault from brain_objects.

    Operates only on the fixed ``vault/`` directory at the project root.

    Steps:
    1. Query all indexed brain_objects.
    2. Group by kind AND market -> create folders ``kind/market_key/``.
    3. For each object, write ``{object_id}.md`` file with:
       - YAML frontmatter: id, kind, provenance_hash, source, trust, ingested_ts
       - Title from summary or kind+id
       - Wikilinks: [[other-object-id]] for linked_events
       - Exclusion: skip raw email bodies, skip low-trust inbox beyond summary
    4. Write ``.gitignore`` (``*``) and ``.gitkeep`` in vault root.
    """
    vault_root = Path(_VAULT_DIR)

    # 1. Query all objects
    objects = await brain_store.list_objects()

    # 2. Group by kind AND market
    kind_market_groups: dict[str, dict[str, list[BrainObject]]] = {}
    for obj in objects:
        if not _should_include_object(obj):
            continue
        kind = obj.kind.value if hasattr(obj.kind, "value") else str(obj.kind)
        if kind not in kind_market_groups:
            kind_market_groups[kind] = {}
        markets = obj.market_keys if obj.market_keys else ["_no_market"]
        for market in markets:
            kind_market_groups[kind].setdefault(market, []).append(obj)

    # Remove and recreate vault root (clean generation)
    if vault_root.exists():
        shutil.rmtree(vault_root)
    vault_root.mkdir(parents=True, exist_ok=True)

    # 3. Write .gitignore (ignores everything except .gitkeep)
    gitignore_path = vault_root / ".gitignore"
    gitignore_path.write_text("*\n", encoding="utf-8")
    logger.debug("vault: wrote .gitignore at %s", gitignore_path.resolve())

    # 4. Write .gitkeep
    gitkeep = vault_root / ".gitkeep"
    gitkeep.write_text("", encoding="utf-8")

    # 5. Write .md files per kind/market
    total_count = 0
    for kind, market_groups in sorted(kind_market_groups.items()):
        kind_dir = vault_root / kind
        kind_dir.mkdir(parents=True, exist_ok=True)

        for market, market_objects in sorted(market_groups.items()):
            market_dir = kind_dir / market
            market_dir.mkdir(parents=True, exist_ok=True)

            for obj in market_objects:
                frontmatter = _yaml_frontmatter(obj)
                body = _body_content(obj)
                filename = f"{obj.id}.md"
                filepath = market_dir / filename

                filepath.write_text(f"{frontmatter}\n{body}\n", encoding="utf-8")
                total_count += 1

    logger.info(
        "vault: regenerated with %d objects across %d kinds at %s",
        total_count,
        len(kind_market_groups),
        vault_root.resolve(),
    )
