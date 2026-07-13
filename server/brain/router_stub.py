"""Local EP-202 router-contract stub used by Brain v1.

The implementation deliberately lives behind one interface so EP-202 can replace
it without changing Brain pipeline or recall code.  Feature hashing preserves
token overlap, unlike digest-seeded random vectors, while remaining deterministic.
"""

import hashlib
import math
import os
import re

_TOKEN_RE = re.compile(r"[a-z0-9]+")


def embed_text(text: str, dimensions: int | None = None) -> list[float]:
    """Return a deterministic, normalized feature-hashed text embedding."""
    dims = dimensions or int(os.environ.get("AETHER_EMBED__DIMS", "1024"))
    if dims <= 0:
        raise ValueError("embedding dimensions must be positive")

    vector = [0.0] * dims
    tokens = _TOKEN_RE.findall(text.lower())
    features = tokens + [f"{a}_{b}" for a, b in zip(tokens, tokens[1:], strict=False)]
    for feature in features:
        digest = hashlib.sha256(feature.encode("utf-8")).digest()
        index = int.from_bytes(digest[:8], "big") % dims
        sign = 1.0 if digest[8] & 1 else -1.0
        vector[index] += sign

    norm = math.sqrt(sum(value * value for value in vector))
    return [value / norm for value in vector] if norm else vector
