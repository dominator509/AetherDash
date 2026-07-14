"""Health check for the OpenBB venue adapter.

Exposes an HTTP health endpoint (separate from the gRPC port) so that
orchestration (Docker health checks, K8s probes, systemd `HealthCheck`) can
verify liveness without a gRPC client.
"""

from __future__ import annotations

import json
import logging
import os
from collections.abc import Callable
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from typing import Any

logger = logging.getLogger(__name__)

_HEALTH_PORT = int(os.environ.get("AETHER_VENUE__OPENBB_HEALTH_PORT", "8088"))


def _make_health_handler(
    probe_fn: Callable[[], bool],
) -> type[BaseHTTPRequestHandler]:
    """Factory: create a ``BaseHTTPRequestHandler`` subclass bound to
    ``probe_fn``.

    The handler responds:
    - ``200 OK`` with ``{"status": "ok"}`` when the probe returns ``True``.
    - ``503 Service Unavailable`` with ``{"status": "degraded"}`` otherwise.
    """

    class HealthHandler(BaseHTTPRequestHandler):
        """Minimal HTTP health check handler."""

        # Suppress per-request stderr logging from the base class.
        # All server logs go through the ``logger`` instead.
        def log_message(self, fmt: str, *args: Any) -> None:  # noqa: ANN401
            logger.debug("health: " + fmt, *args)

        def do_GET(self) -> None:  # noqa: N802
            if self.path == "/metrics":
                healthy = probe_fn()
                body = (
                    "# HELP aether_venue_up Whether the OpenBB provider probe succeeds.\n"
                    "# TYPE aether_venue_up gauge\n"
                    f'aether_venue_up{{venue="openbb"}} {1 if healthy else 0}\n'
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/plain; version=0.0.4")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            if self.path not in {"/health", "/healthz", "/readyz"}:
                self.send_response(404)
                self.end_headers()
                self.wfile.write(b"Not Found")
                return

            healthy = probe_fn()
            if healthy:
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"status": "ok"}).encode())
            else:
                self.send_response(503)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"status": "degraded"}).encode())

    return HealthHandler


def start_health_server(probe_fn: Callable[[], bool]) -> HTTPServer:
    """Start a background HTTP health-check server.

    The server listens on ``127.0.0.1:{AETHER_VENUE__OPENBB_HEALTH_PORT}``
    (default 8088).

    Parameters
    ----------
    probe_fn : () -> bool
        A callable that returns ``True`` if the adapter is healthy,
        ``False`` otherwise.

    Returns
    -------
    HTTPServer
        The started server (call ``.shutdown()`` to stop).
    """
    handler_cls = _make_health_handler(probe_fn)
    server = HTTPServer(("127.0.0.1", _HEALTH_PORT), handler_cls)

    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    logger.info("Health server started on 127.0.0.1:%d/health", _HEALTH_PORT)
    return server
