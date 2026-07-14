"""Filing module — submits parsed content to Brain.Store.

Creates an ``ObjectDraft`` with the cleaned text and source metadata,
then calls the Brain service to ingest it.  All inbox-filed objects
are tagged with ``origin=inbox`` and ``trust=low``.
"""

import logging

logger = logging.getLogger(__name__)


async def file_to_brain(
    kind: str,
    source: str,
    raw_bytes: bytes,
    cleaned_text: str,
) -> str:
    """File parsed content to Brain.Store.

    Creates an ``ObjectDraft`` with the provided metadata and submits it
    to the Brain ingestion pipeline.  The resulting BrainObject gets
    ``origin=inbox`` and ``trust=low``.

    Args:
        kind: Object kind (``"email"``, ``"document"``, ``"screenshot"``).
        source: Originating address / identifier (from-address).
        raw_bytes: Original raw content bytes.
        cleaned_text: Parsed / cleaned text content.

    Returns:
        The Brain object ID (ULID).
    """
    # Lazy import to avoid pulling in full brain stack at module level
    from server.brain import service as brain_service  # noqa: PLC0415
    from server.brain.models import ObjectDraft  # noqa: PLC0415

    draft = ObjectDraft(
        kind=kind,
        content=cleaned_text,
        source=source,
    )

    ref = await brain_service.store_draft(
        draft,
        origin="inbox",
        trust="low",
        raw_content=raw_bytes,
    )

    logger.debug(
        "Filed to brain: kind=%s source=%s brain_id=%s",
        kind,
        source,
        ref.id,
    )
    return ref.id
