"""Twilio webhook authentication helpers."""

from __future__ import annotations

import base64
import hashlib
import hmac
from collections.abc import Mapping


def expected_signature(url: str, params: Mapping[str, str], auth_token: str) -> str:
    """Compute Twilio's HMAC-SHA1 request signature."""
    material = url + "".join(key + params[key] for key in sorted(params))
    digest = hmac.new(auth_token.encode(), material.encode(), hashlib.sha1).digest()
    return base64.b64encode(digest).decode()


def verify_signature(
    url: str, params: Mapping[str, str], supplied: str, auth_token: str
) -> bool:
    if not url or not supplied or not auth_token:
        return False
    return hmac.compare_digest(expected_signature(url, params, auth_token), supplied)
