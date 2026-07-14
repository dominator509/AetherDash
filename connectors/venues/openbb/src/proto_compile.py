"""Proto compilation at import time.

Compiles the ``aether.venue.v1.VenueAdapter`` proto and all its dependency
protos into Python stubs, following the same pattern as the Brain service
(``server/brain/grpc_server.py``).

The compiled modules are placed in a temporary directory on ``sys.path`` and
re-exported as module-level globals so the rest of the pack can do::

    from .proto_compile import venue_pb2, venue_pb2_grpc, core_pb2, market_data_pb2
"""

from __future__ import annotations

import logging
import os
import sys
import tempfile
from pathlib import Path
from types import ModuleType

from grpc_tools import protoc

logger = logging.getLogger(__name__)

# ── Proto discovery ────────────────────────────────────────────────────────

_PROJECT_ROOT = (
    Path(__file__).resolve().parents[4]
)  # src/ -> openbb/ -> venues/ -> connectors/ -> repo root
_PROTO_DIR = _PROJECT_ROOT / "proto"

_VENUE_PROTO = _PROTO_DIR / "aether" / "venue" / "v1" / "adapter.proto"
_CORE_PROTO_DIR = str(_PROTO_DIR)


def _compile_protos() -> tuple:
    """Compile all relevant protos and return the generated modules.

    Returns
    -------
    tuple
        ``(venue_pb2, venue_pb2_grpc, core_pb2, market_data_pb2)``
    """
    # Locate the protobuf well-known-types include directory
    import grpc_tools  # noqa: PLC0415

    grpc_tools_dir = os.path.dirname(os.path.abspath(grpc_tools.__file__))
    protobuf_include = os.path.join(grpc_tools_dir, "_proto")

    # Collect all .proto files under the project proto dir so that
    # dependency stubs (aether.core.v1.*) are also generated.
    proto_files: list[str] = []
    for root, _dirs, files in os.walk(_CORE_PROTO_DIR):
        for fname in files:
            if fname.endswith(".proto"):
                proto_files.append(os.path.join(root, fname))

    with tempfile.TemporaryDirectory() as tmpdir:
        result = protoc.main(
            [
                "grpc_tools.protoc",
                f"-I{_CORE_PROTO_DIR}",
                f"-I{protobuf_include}",
                f"--python_out={tmpdir}",
                f"--grpc_python_out={tmpdir}",
                *proto_files,
            ]
        )
        if result != 0:
            raise RuntimeError(f"grpc_tools.protoc failed with exit code {result}")
        sys.path.insert(0, tmpdir)

        # Import the generated venue proto stubs
        # Import market data types (Market, Quote, InstrumentKind, etc.)
        import aether.core.v1.market_data_pb2 as _market_data_pb2  # type: ignore[import-untyped] # noqa: PLC0415

        # Import core types
        import aether.core.v1.types_pb2 as _types_pb2  # type: ignore[import-untyped] # noqa: PLC0415
        import aether.venue.v1.adapter_pb2 as _venue_pb2  # type: ignore[import-untyped] # noqa: PLC0415
        import aether.venue.v1.adapter_pb2_grpc as _venue_pb2_grpc  # type: ignore[import-untyped] # noqa: PLC0415

        sys.path.pop(0)

        # Wrap core_pb2 as a namespace so callers can access MarketKey, etc.
        # through a single import.
        import types  # noqa: PLC0415

        _core_pb2 = types.ModuleType("core_pb2")
        _core_pb2.MarketKey = _types_pb2.MarketKey
        _core_pb2.VenueId = _types_pb2.VenueId
        _core_pb2.Ulid = _types_pb2.Ulid
        _core_pb2.Money = _types_pb2.Money

        logger.info(
            "Compiled %d proto files into temporary Python stubs",
            len(proto_files),
        )

        return _venue_pb2, _venue_pb2_grpc, _core_pb2, _market_data_pb2


# Module-level exports — populated once at import time
venue_pb2: ModuleType = None  # type: ignore[assignment]
venue_pb2_grpc: ModuleType = None  # type: ignore[assignment]
core_pb2: ModuleType = None  # type: ignore[assignment]
market_data_pb2: ModuleType = None  # type: ignore[assignment]

(
    venue_pb2,
    venue_pb2_grpc,
    core_pb2,
    market_data_pb2,
) = _compile_protos()
