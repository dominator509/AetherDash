"""Safe PDF text extraction.

Resource-limited: rejects inputs larger than 10 MB and enforces a 30-second
wall-clock timeout on extraction.  Never executes JavaScript, never makes
external HTTP calls — PyPDF2 is a pure-text-extraction library with no
JS interpreter.
"""

import io
import logging
from concurrent.futures import ThreadPoolExecutor
from concurrent.futures import TimeoutError as FuturesTimeout

import pypdf

logger = logging.getLogger(__name__)

_MAX_INPUT_SIZE = 10 * 1024 * 1024  # 10 MB
_MAX_PROCESSING_SECONDS = 30


def _extract_text_sync(raw_bytes: bytes) -> str:
    """Run PyPDF2 text extraction in a synchronous worker thread."""
    reader = pypdf.PdfReader(io.BytesIO(raw_bytes))
    pages: list[str] = []
    for page in reader.pages:
        text = page.extract_text()
        if text:
            pages.append(text)
    return "\n".join(pages)


def parse_pdf(raw_bytes: bytes) -> str:
    """Extract text from a PDF with resource limits.

    Args:
        raw_bytes: Raw PDF file content.

    Returns:
        Extracted text content.

    Raises:
        ValueError: If the input exceeds 10 MB.
        TimeoutError: If extraction takes longer than 30 seconds.
        pypdf.errors.PdfReadError: If the PDF is corrupt or unreadable.
    """
    if len(raw_bytes) > _MAX_INPUT_SIZE:
        msg = f"PDF input too large: {len(raw_bytes)} bytes (max {_MAX_INPUT_SIZE})"
        raise ValueError(msg)

    pool = ThreadPoolExecutor(max_workers=1)
    try:
        future = pool.submit(_extract_text_sync, raw_bytes)
        try:
            return future.result(timeout=_MAX_PROCESSING_SECONDS)
        except FuturesTimeout:
            future.cancel()
            msg = f"PDF extraction timed out after {_MAX_PROCESSING_SECONDS}s"
            raise TimeoutError(msg) from None
    finally:
        # Do not block past the declared wall-clock limit waiting for a hostile
        # parser job. The process/container supervisor owns final reclamation.
        pool.shutdown(wait=False, cancel_futures=True)
