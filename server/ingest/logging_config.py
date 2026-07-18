"""Structured, body-free service logging."""

import logging
import os

import structlog


def configure_logging() -> None:
    json_logs = os.environ.get("AETHER_LOG__FORMAT", "console") == "json"
    level_name = os.environ.get("AETHER_LOG__LEVEL", "INFO").upper()
    level = getattr(logging, level_name, logging.INFO)
    processors = [
        structlog.contextvars.merge_contextvars,
        structlog.processors.add_log_level,
        structlog.processors.TimeStamper(fmt="iso", key="ts", utc=True),
    ]
    renderer = (
        structlog.processors.JSONRenderer()
        if json_logs
        else structlog.dev.ConsoleRenderer(colors=False)
    )
    logging.basicConfig(level=level)
    structlog.configure(
        processors=[*processors, renderer],
        wrapper_class=structlog.make_filtering_bound_logger(level),
        cache_logger_on_first_use=True,
    )
