"""gRPC Brain service server.

Compiles the proto definition at import time via ``grpcio_tools.protoc``
so the proto stays the single source of truth (ADR-0006).
"""

import logging
import os
import sys
import tempfile
import types
from concurrent import futures
from pathlib import Path

import grpc
from grpc_tools import protoc

from server.brain import service as brain_service
from server.brain.models import BrainRef
from server.brain.models import ObjectDraft as ObjectDraftModel

logger = logging.getLogger(__name__)

# ── Proto compilation ────────────────────────────────────────────────────

_PROTO_DIR = Path(__file__).resolve().parents[2] / "proto"
_BRAIN_PROTO = _PROTO_DIR / "aether" / "brain" / "v1" / "brain.proto"
_CORE_PROTO_DIR = str(_PROTO_DIR)

# Compiled proto modules — populated at module load
brain_pb2 = None  # type: ignore
brain_pb2_grpc = None  # type: ignore
core_pb2 = None  # type: ignore  # aether.core.v1.opportunity_pb2 (for BrainRef, Ulid)


def _compile_proto() -> None:
    """Compile the Brain proto into Python stubs at import time.

    Compiles the brain proto AND its dependency protos so that all
    generated Python modules (brain_pb2, core_pb2, etc.) are available
    in a single temp directory on ``sys.path``.
    """
    global brain_pb2, brain_pb2_grpc, core_pb2  # noqa: PLW0603

    # Locate the protobuf well-known-types include directory bundled with
    # grpc_tools so that ``import "google/protobuf/timestamp.proto"``
    # and similar standard imports resolve correctly.
    import grpc_tools  # noqa: PLC0415

    _grpc_tools_dir = os.path.dirname(os.path.abspath(grpc_tools.__file__))
    _protobuf_include = os.path.join(_grpc_tools_dir, "_proto")

    # Collect all .proto files under the project proto dir so that
    # dependency stubs (aether.core.v1.*) are also generated.
    _proto_files: list[str] = []
    for root, _dirs, files in os.walk(_CORE_PROTO_DIR):
        for fname in files:
            if fname.endswith(".proto"):
                _proto_files.append(os.path.join(root, fname))

    with tempfile.TemporaryDirectory() as tmpdir:
        protoc.main(
            [
                "grpc_tools.protoc",
                f"-I{_CORE_PROTO_DIR}",
                f"-I{_protobuf_include}",
                f"--python_out={tmpdir}",
                f"--grpc_python_out={tmpdir}",
                *_proto_files,
            ]
        )
        sys.path.insert(0, tmpdir)
        import aether.brain.v1.brain_pb2 as pb2  # type: ignore # noqa: PLC0415
        import aether.brain.v1.brain_pb2_grpc as pb2_grpc  # type: ignore # noqa: PLC0415

        # Import core proto types (always generated as dependencies)
        try:
            import aether.core.v1.opportunity_pb2 as opportunity_pb2  # type: ignore # noqa: PLC0415
            import aether.core.v1.types_pb2 as types_pb2  # type: ignore # noqa: PLC0415

            # Proxy core_pb2 through a SimpleNamespace so callers can access
            # both BrainRef (from opportunity_pb2) and Ulid (from types_pb2)
            # via ``core_pb2.BrainRef`` and ``core_pb2.Ulid``.
            core_pb2 = types.SimpleNamespace(
                BrainRef=opportunity_pb2.BrainRef,
                Ulid=types_pb2.Ulid,
            )  # type: ignore
        except ImportError:
            # Fallback: use brain_pb2's symbol table (protoc may inline deps)
            core_pb2 = pb2  # type: ignore

        sys.path.pop(0)

        brain_pb2 = pb2
        brain_pb2_grpc = pb2_grpc


_compile_proto()

# ── gRPC Servicer ────────────────────────────────────────────────────────

_BIND = os.environ.get("AETHER_BRAIN__BIND", "127.0.0.1:8000")


class BrainServicer(brain_pb2_grpc.BrainServicer):  # type: ignore
    """Implements the Brain gRPC service."""

    async def Store(  # noqa: N802
        self, request, context
    ):
        """Store an ObjectDraft and return a BrainRef."""
        draft = ObjectDraftModel(
            kind=request.kind,
            content=request.content,
            source=request.source,
        )
        ref = await brain_service.store_draft(draft)
        return core_pb2.BrainRef(
            object_id=core_pb2.Ulid(value=ref.id),
            provenance_hash=ref.provenance_hash,
        )

    async def Get(  # noqa: N802
        self, request, context
    ):
        """Get an ObjectDraft by BrainRef."""
        ref = BrainRef(
            id=request.id,
            provenance_hash=request.provenance_hash,
        )
        obj = await brain_service.get(ref)
        if obj is None:
            await context.abort(grpc.StatusCode.NOT_FOUND, f"object {ref.id} not found")
        return brain_pb2.ObjectDraft(
            kind=obj.kind.value if obj else "",
            content=obj.summary or "",
            source=obj.source if obj else "",
        )

    async def Recall(  # noqa: N802
        self, request, context
    ):
        """Recall v1 — hybrid RRF retrieval.

        Fetches from Qdrant and Postgres FTS, fuses results via RRF,
        returns a list of ScoredRef with provenance hashes.
        """
        import json  # noqa: PLC0415

        filters = {}
        if request.filters:
            try:
                filters = json.loads(request.filters)
            except json.JSONDecodeError:
                await context.abort(
                    grpc.StatusCode.INVALID_ARGUMENT,
                    f"filters must be valid JSON, got: {request.filters!r}",
                )

        try:
            scored_refs = await brain_service.recall(
                query=request.query,
                k=int(request.k) if request.k else 24,
                filters=filters,
            )
        except Exception:
            logger.exception("Recall failed")
            await context.abort(
                grpc.StatusCode.INTERNAL,
                "Recall failed — internal error",
            )

        # Build proto response
        pb_refs = []
        for sr in scored_refs:
            pb_refs.append(
                brain_pb2.ScoredRef(
                    ref=core_pb2.BrainRef(
                        object_id=core_pb2.Ulid(value=sr.object_id),
                        provenance_hash=sr.provenance_hash,
                    ),
                    score=sr.score,
                )
            )

        return brain_pb2.RecallResponse(refs=pb_refs)

    async def Explain(  # noqa: N802
        self, request, context
    ):
        """Build an ExplainTree for an opportunity."""
        from server.brain.explain import explain as build_explain  # noqa: PLC0415

        opportunity_id = (
            request.opportunity_id.value
            if hasattr(request, "opportunity_id")
            and hasattr(request.opportunity_id, "value")
            else str(request.opportunity_id)
        )
        tree = await build_explain(opportunity_id)
        if tree is None:
            await context.abort(
                grpc.StatusCode.NOT_FOUND,
                f"opportunity {opportunity_id} not found",
            )
        import json  # noqa: PLC0415

        return brain_pb2.ExplainTree(tree_json=json.dumps(tree))


# ── Server lifecycle ────────────────────────────────────────────────────


async def serve_grpc() -> grpc.aio.Server:
    """Start the gRPC server and return the server object."""
    server = grpc.aio.server(futures.ThreadPoolExecutor(max_workers=10))
    brain_pb2_grpc.add_BrainServicer_to_server(BrainServicer(), server)
    server.add_insecure_port(_BIND)
    await server.start()
    logger.info("Brain gRPC server started on %s", _BIND)
    return server
