"""Ingestion pipeline — 7-stage clean→summarize→extract→link→embed→index.

Each stage is idempotent and resumable. Stages emit ``ingest_events`` rows
via ClickHouse (best-effort) with ``ladder_rung`` corresponding to the stage:
    1 = intake (service.py)
    2 = clean
    3 = summarize
    4 = extract
    5 = link
    6 = embed
    7 = index
"""

from server.brain.pipeline.runner import run_pipeline, run_pipeline_sync

__all__ = ["run_pipeline", "run_pipeline_sync"]
