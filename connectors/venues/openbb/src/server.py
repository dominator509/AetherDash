"""OpenBB venue adapter — gRPC server entrypoint.

Usage
-----
Run directly::

    python -m connectors.venues.openbb.src.server

Or via the installed entry point (once packaged)::

    aether-venue-openbb

Environment variables
---------------------
See ``aether-blueprint/ENVIRONMENT.md`` for the full contract.

Key variables:
- ``AETHER_VENUE__OPENBB_GRPC_ADDR`` — gRPC bind address (default: ``127.0.0.1:50058``)
- ``AETHER_VENUE__OPENBB_HEALTH_PORT`` — HTTP health port (default: ``8088``)
- ``AETHER_VENUE__OPENBB_PROVIDER`` — OpenBB data provider (default: ``yfinance``)
- ``AETHER_VENUE__OPENBB_POLL_INTERVAL_SECS`` — tick poll interval (default: ``5``)
- ``AETHER_VENUE__OPENBB_WATCHLIST`` — comma-separated symbol watchlist
"""

from __future__ import annotations

import logging
import os
from concurrent import futures

import grpc
from grpc_health.v1 import health, health_pb2, health_pb2_grpc

from .adapter import OpenbbVenueAdapter
from .health import start_health_server
from .proto_compile import venue_pb2_grpc

logger = logging.getLogger(__name__)

_GRPC_ADDR = os.environ.get("AETHER_VENUE__OPENBB_GRPC_ADDR", "127.0.0.1:50058")


def serve() -> grpc.Server:
    """Start the gRPC server and the HTTP health endpoint.

    Returns
    -------
    grpc.Server
        The started gRPC server (call ``.wait_for_termination()`` to block).
    """
    # ── gRPC server ────────────────────────────────────────────────────
    server = grpc.server(
        futures.ThreadPoolExecutor(max_workers=10),
        # Max message size: 4 MB (matching the proto contract default)
        options=[
            ("grpc.max_send_message_length", 4 * 1024 * 1024),
            ("grpc.max_receive_message_length", 4 * 1024 * 1024),
        ],
    )
    adapter = OpenbbVenueAdapter()
    venue_pb2_grpc.add_VenueAdapterServicer_to_server(adapter, server)
    grpc_health = health.HealthServicer()
    health_pb2_grpc.add_HealthServicer_to_server(grpc_health, server)
    grpc_health.set("", health_pb2.HealthCheckResponse.SERVING)
    server.add_insecure_port(_GRPC_ADDR)
    server.start()
    logger.info("OpenBB gRPC server listening on %s", _GRPC_ADDR)

    # ── HTTP health server ─────────────────────────────────────────────
    start_health_server(adapter.is_ready)
    logger.info("OpenBB health endpoint started")

    return server


def main() -> None:
    """CLI entry point."""
    logging.basicConfig(
        level=os.environ.get("AETHER_LOG__LEVEL", "INFO").upper(),
        format=(
            "%(asctime)s [%(levelname)s] %(name)s: %(message)s"
            if os.environ.get("AETHER_LOG__FORMAT") != "json"
            else "%(message)s"
        ),
    )
    logger.info("Starting AETHER Venue — OpenBB adapter")

    server = serve()
    try:
        server.wait_for_termination()
    except KeyboardInterrupt:
        logger.info("Shutting down OpenBB adapter")
        server.stop(grace=5)


if __name__ == "__main__":
    main()
