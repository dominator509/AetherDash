"""Plain text parser with a bounded input size."""

_MAX_INPUT_SIZE = 10 * 1024 * 1024


def parse_text(raw_bytes: bytes) -> str:
    """Decode raw bytes as UTF-8 text.

    Args:
        raw_bytes: UTF-8 encoded text bytes.

    Returns:
        Decoded string.
    """
    if len(raw_bytes) > _MAX_INPUT_SIZE:
        raise ValueError("text input exceeds 10 MiB limit")
    return raw_bytes.decode("utf-8")
