"""Redpanda adapter for the alert service's registered bus topics."""

import json
import os
from collections.abc import AsyncIterator


class AlertBus:
    """Small aiokafka adapter kept behind the EP-004 topic contract."""

    def __init__(self) -> None:
        self._bootstrap = os.environ.get("AETHER_KAFKA_BOOTSTRAP", "localhost:9092")
        self._consumer = None
        self._producer = None

    async def start(self) -> None:
        from aiokafka import AIOKafkaConsumer, AIOKafkaProducer

        self._consumer = AIOKafkaConsumer(
            "opps.detected",
            bootstrap_servers=self._bootstrap,
            group_id="svc.alerts",
            enable_auto_commit=False,
            value_deserializer=lambda value: json.loads(value.decode()),
        )
        self._producer = AIOKafkaProducer(
            bootstrap_servers=self._bootstrap,
            value_serializer=lambda value: json.dumps(
                value, separators=(",", ":")
            ).encode(),
        )
        await self._producer.start()
        try:
            await self._consumer.start()
        except Exception:
            await self._producer.stop()
            self._producer = None
            raise

    async def stop(self) -> None:
        if self._consumer is not None:
            await self._consumer.stop()
        if self._producer is not None:
            await self._producer.stop()
        self._consumer = self._producer = None

    async def opportunities(self) -> AsyncIterator[dict]:
        if self._consumer is None:
            raise RuntimeError("bus is not started")
        async for message in self._consumer:
            envelope = message.value
            yield envelope.get("payload", envelope)
            await self._consumer.commit()

    async def publish(self, topic: str, envelope: dict) -> None:
        if self._producer is None:
            raise RuntimeError("bus is not started")
        await self._producer.send_and_wait(topic, envelope)
