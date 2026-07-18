"""Bounded OCR that re-files image text through the existing Brain pipeline."""

import asyncio
import io
import logging
import os
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Any, Protocol

import numpy as np
from PIL import Image

from server.brain import storage, store
from server.brain.models import BrainObject, BrainRef, ObjectDraft, ObjectKind
from server.brain.pipeline import runner
from server.brain.service import store_draft

logger = logging.getLogger(__name__)

_MAX_INPUT_BYTES = 20 * 1024 * 1024
_MAX_PIXELS = 40_000_000
_PENDING_MARKER = "OCR pending"


@dataclass(frozen=True)
class OcrResult:
    text: str
    mean_confidence: float
    engine: str


class OcrEngine(Protocol):
    name: str

    def recognize(self, raw_image: bytes) -> OcrResult: ...


class CpuRapidOcrEngine:
    name = "rapidocr-onnxruntime-cpu"

    def __init__(self, engine: Any | None = None) -> None:
        if engine is None:
            from rapidocr import RapidOCR  # noqa: PLC0415

            engine = RapidOCR()
        self._engine = engine

    def recognize(self, raw_image: bytes) -> OcrResult:
        image = _validated_rgb_image(raw_image)
        output = self._engine(np.asarray(image))
        texts = tuple(getattr(output, "txts", ()) or ())
        scores = tuple(float(value) for value in (getattr(output, "scores", ()) or ()))
        accepted = [
            (text.strip(), scores[index] if index < len(scores) else 0.0)
            for index, text in enumerate(texts)
            if isinstance(text, str) and text.strip()
        ]
        text = "\n".join(value for value, _ in accepted)
        confidence = (
            sum(score for _, score in accepted) / len(accepted) if accepted else 0.0
        )
        return OcrResult(text=text, mean_confidence=confidence, engine=self.name)


def _validated_rgb_image(raw_image: bytes) -> Image.Image:
    if not raw_image or len(raw_image) > _MAX_INPUT_BYTES:
        raise ValueError("OCR image must be between 1 byte and 20 MiB")
    try:
        image = Image.open(io.BytesIO(raw_image))
        width, height = image.size
        if width < 1 or height < 1 or width * height > _MAX_PIXELS:
            raise ValueError("OCR image dimensions exceed the 40 megapixel limit")
        image.load()
        return image.convert("RGB")
    except ValueError:
        raise
    except Exception as exc:
        raise ValueError("OCR input is not a supported image") from exc


def build_ocr_engine(mode: str | None = None) -> OcrEngine:
    selected = (mode or os.environ.get("AETHER_INGEST__OCR_ENGINE", "cpu")).lower()
    if selected == "cpu":
        return CpuRapidOcrEngine()
    if selected != "gpu":
        raise ValueError("AETHER_INGEST__OCR_ENGINE must be cpu or gpu")
    try:
        from server.ingest.ocr.gpu_worker import GpuRapidOcrEngine  # noqa: PLC0415

        return GpuRapidOcrEngine()
    except Exception as exc:
        logger.warning(
            "GPU OCR unavailable; using the CPU fallback: %s", type(exc).__name__
        )
        return CpuRapidOcrEngine()


StoreDraft = Callable[..., Awaitable[BrainRef]]


class OcrPipeline:
    def __init__(
        self,
        engine: OcrEngine,
        *,
        brain_store_draft: StoreDraft = store_draft,
    ) -> None:
        self.engine = engine
        self.brain_store_draft = brain_store_draft

    async def recognize_and_store(
        self, raw_image: bytes, *, source: str, ladder_rung: int
    ) -> tuple[BrainRef, OcrResult]:
        result = await asyncio.to_thread(self.engine.recognize, raw_image)
        if not result.text:
            raise ValueError("OCR found no text in the image")
        ref = await self.brain_store_draft(
            ObjectDraft(kind="screenshot", content=result.text, source=source),
            origin="ingest_fleet",
            raw_content=raw_image,
            ladder_rung=ladder_rung,
        )
        return ref, result

    async def reprocess_existing(self, object_id: str) -> OcrResult:
        obj = await store.get_object(object_id)
        if obj is None or obj.kind != ObjectKind.screenshot or not obj.raw_ref:
            raise ValueError("OCR reprocessing requires a stored screenshot object")
        raw_image = await asyncio.to_thread(storage.get_raw, obj.raw_ref)
        result = await asyncio.to_thread(self.engine.recognize, raw_image)
        if not result.text:
            raise ValueError("OCR found no text in the image")
        _, clean_ref = await asyncio.to_thread(
            storage.store_clean, result.text.encode(), obj.source
        )
        await store.update_object(
            object_id,
            clean_ref=clean_ref,
            summary=None,
            entities=[],
            linked_events=[],
            market_keys=[],
            confidence=result.mean_confidence,
            tier="warm",
            current_stage="intake",
            parked_reason=None,
        )
        await runner.run_pipeline(object_id)
        return result

    async def pending_screenshots(self, limit: int = 25) -> list[BrainObject]:
        candidates = await store.list_objects(kind_filter=[ObjectKind.screenshot.value])
        pending: list[BrainObject] = []
        for obj in candidates:
            if len(pending) >= limit or not obj.clean_ref:
                continue
            clean = await asyncio.to_thread(storage.get_clean, obj.clean_ref)
            if _PENDING_MARKER in clean.decode("utf-8", errors="replace"):
                pending.append(obj)
        return pending
