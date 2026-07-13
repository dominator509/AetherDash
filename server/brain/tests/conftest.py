"""Test configuration for brain integration tests."""

import os

import pytest
import pytest_asyncio


def _infra_available() -> bool:
    """Quick check if required infrastructure ports are reachable."""
    import socket

    hosts = {
        "postgres": (os.environ.get("AETHER_MINIO__ENDPOINT", "localhost"), 5432),
        "minio": ("localhost", 9000),
        "qdrant": ("localhost", 6333),
    }
    for _name, (host, port) in hosts.items():
        # Strip protocol prefix from host
        for prefix in ("https://", "http://"):
            if host.startswith(prefix):
                host = host[len(prefix) :]
                break
        try:
            with socket.create_connection((host, port), timeout=1.0):
                pass
        except (TimeoutError, ConnectionRefusedError, OSError):
            return False
    return True


# Skip all integration tests if infrastructure is not available
skip_integration = pytest.mark.skipif(
    not _infra_available(),
    reason="requires dev stack (MinIO :9000, Postgres :5432)",
)


@pytest_asyncio.fixture
async def clean_brain_objects():
    """Clean the brain_objects table before and after each test."""
    from server.brain import store as brain_store

    pool = await brain_store.get_pool()
    async with pool.acquire() as conn:
        await conn.execute("DELETE FROM brain_objects")
    yield
    async with pool.acquire() as conn:
        await conn.execute("DELETE FROM brain_objects")
    await brain_store.close_pool()
