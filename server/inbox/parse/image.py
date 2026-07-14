"""Bounded image parking parser — placeholder for OCR (EP-206).

Stores the raw image size and returns a placeholder string.  Actual
OCR will be implemented in EP-206 (Ingestion fleet, OCR).
"""

_MAX_INPUT_SIZE = 20 * 1024 * 1024


def parse_image(raw_bytes: bytes) -> str:
    """Return a placeholder string for image content.

    Args:
        raw_bytes: Raw image file bytes.

    Returns:
        Placeholder text noting the image size and that OCR is pending.
    """
    size = len(raw_bytes)
    if size > _MAX_INPUT_SIZE:
        raise ValueError("image input exceeds 20 MiB limit")
    return f"[Image: {size} bytes, OCR pending — EP-206]"
