"""Prometheus projection from durable ingestion audit state."""

import asyncpg


def _label(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")


async def render_metrics(pool: asyncpg.Pool) -> str:
    counts = await pool.fetch(
        """
        SELECT source,ladder_rung,count(*) AS count
        FROM ingest_source_events GROUP BY source,ladder_rung
        ORDER BY source,ladder_rung
        """
    )
    health = await pool.fetch(
        """
        SELECT source,ladder_rung,health,consecutive_failures
        FROM ingest_source_state ORDER BY source
        """
    )
    lines = [
        "# HELP aether_ingest_objects_total Ingested Brain objects by source compliance rung",
        "# TYPE aether_ingest_objects_total counter",
    ]
    for row in counts:
        source = _label(row["source"])
        rung = row["ladder_rung"]
        count = row["count"]
        lines.append(
            f'aether_ingest_objects_total{{source="{source}",'
            f'ladder_rung="{rung}"}} {count}'
        )
    lines.extend(
        [
            "# HELP aether_ingest_source_healthy Whether a configured source is healthy",
            "# TYPE aether_ingest_source_healthy gauge",
            "# HELP aether_ingest_source_failures Consecutive source failures",
            "# TYPE aether_ingest_source_failures gauge",
        ]
    )
    for row in health:
        source = _label(row["source"])
        rung = row["ladder_rung"]
        labels = f'source="{source}",ladder_rung="{rung}"'
        lines.append(
            f"aether_ingest_source_healthy{{{labels}}} "
            f"{1 if row['health'] == 'healthy' else 0}"
        )
        lines.append(
            f"aether_ingest_source_failures{{{labels}}} {row['consecutive_failures']}"
        )
    lines.extend(
        [
            "# HELP aether_build_info Build information for the service",
            "# TYPE aether_build_info gauge",
            'aether_build_info{service="ingest",version="0.1.0"} 1',
            "",
        ]
    )
    return "\n".join(lines)
