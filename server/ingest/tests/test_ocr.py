import asyncio
import io
import os
from unittest.mock import AsyncMock

import pytest
from PIL import Image, ImageDraw, ImageFont

from server.brain import store
from server.brain.models import (
    BrainObject,
    BrainRef,
    ObjectDraft,
    ObjectKind,
    Origin,
    TrustLevel,
)
from server.ingest.ocr import pipeline as ocr_module
from server.ingest.ocr.pipeline import (
    CpuRapidOcrEngine,
    OcrPipeline,
    OcrResult,
    build_ocr_engine,
)


@pytest.fixture(scope="module")
def ocr_fixture_image() -> bytes:
    image = Image.new("RGB", (900, 220), "white")
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default(size=80)
    draw.text((45, 55), "AETHER OCR 2046", fill="black", font=font)
    output = io.BytesIO()
    image.save(output, format="PNG")
    return output.getvalue()


@pytest.mark.slow
def test_cpu_ocr_recognizes_fixture_image(ocr_fixture_image: bytes) -> None:
    result = CpuRapidOcrEngine().recognize(ocr_fixture_image)
    normalized = result.text.upper().replace(" ", "")
    assert "AETHER" in normalized
    assert "2046" in normalized
    assert result.mean_confidence > 0.5
    assert result.engine == "rapidocr-onnxruntime-cpu"


def test_ocr_rejects_non_image_input() -> None:
    engine = CpuRapidOcrEngine(engine=lambda _: None)
    with pytest.raises(ValueError, match="supported image"):
        engine.recognize(b"not an image")


class FixtureEngine:
    name = "fixture-cpu"

    def recognize(self, raw_image: bytes) -> OcrResult:
        assert raw_image == b"fixture image"
        return OcrResult(
            text="recognized filing text", mean_confidence=0.98, engine=self.name
        )


@pytest.mark.asyncio
async def test_ocr_refiles_through_brain_store_with_original_rung() -> None:
    brain_store = AsyncMock(
        return_value=BrainRef(id="01ARZ3NDEKTSV4RRFFQ69G5FAV", provenance_hash="0" * 64)
    )
    pipeline = OcrPipeline(FixtureEngine(), brain_store_draft=brain_store)

    ref, result = await pipeline.recognize_and_store(
        b"fixture image", source="inbox:operator", ladder_rung=5
    )

    assert len(ref.id) == 26
    assert result.text == "recognized filing text"
    draft = brain_store.await_args.args[0]
    assert isinstance(draft, ObjectDraft)
    assert draft.kind == "screenshot"
    assert draft.content == "recognized filing text"
    assert brain_store.await_args.kwargs["raw_content"] == b"fixture image"
    assert brain_store.await_args.kwargs["ladder_rung"] == 5


@pytest.mark.asyncio
async def test_ocr_reprocesses_parked_inbox_screenshot_without_changing_rung(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    obj = BrainObject(
        id="01ARZ3NDEKTSV4RRFFQ69G5FAV",
        kind=ObjectKind.screenshot,
        source="inbox:operator",
        origin=Origin.inbox,
        trust=TrustLevel.low,
        ingested_ts="2026-07-18T00:00:00.000Z",
        raw_ref="raw/inbox/fixture",
        clean_ref="clean/inbox/pending",
        provenance_hash="0" * 64,
        ladder_rung=5,
    )
    update = AsyncMock()
    run_pipeline = AsyncMock()
    monkeypatch.setattr(ocr_module.store, "get_object", AsyncMock(return_value=obj))
    monkeypatch.setattr(ocr_module.store, "update_object", update)
    monkeypatch.setattr(ocr_module.storage, "get_raw", lambda _: b"fixture image")
    monkeypatch.setattr(
        ocr_module.storage,
        "store_clean",
        lambda content, source: ("hash", "clean/inbox/recognized"),
    )
    monkeypatch.setattr(ocr_module.runner, "run_pipeline", run_pipeline)

    result = await OcrPipeline(FixtureEngine()).reprocess_existing(obj.id)

    assert result.text == "recognized filing text"
    assert update.await_args.kwargs["clean_ref"] == "clean/inbox/recognized"
    assert update.await_args.kwargs["current_stage"] == "intake"
    assert "ladder_rung" not in update.await_args.kwargs
    run_pipeline.assert_awaited_once_with(obj.id)


def test_invalid_ocr_engine_config_is_rejected() -> None:
    with pytest.raises(ValueError, match="must be cpu or gpu"):
        build_ocr_engine("remote")


@pytest.mark.integration
@pytest.mark.asyncio
@pytest.mark.slow
@pytest.mark.skipif(
    os.environ.get("AETHER_INTEGRATION_TEST") != "1",
    reason="set AETHER_INTEGRATION_TEST=1 for live Brain/MinIO integration",
)
async def test_cpu_ocr_refiled_object_becomes_recallable(
    ocr_fixture_image: bytes,
) -> None:
    pipeline = OcrPipeline(CpuRapidOcrEngine())
    ref, result = await pipeline.recognize_and_store(
        ocr_fixture_image,
        source="ocr:ep206:integration",
        ladder_rung=5,
    )
    try:
        deadline = asyncio.get_running_loop().time() + 20
        obj = await store.get_object(ref.id)
        while (
            obj is not None
            and obj.current_stage not in {"index"}
            and not obj.current_stage.startswith("parked:")
        ):
            if asyncio.get_running_loop().time() >= deadline:
                pytest.fail("OCR-refiled Brain object did not finish its pipeline")
            await asyncio.sleep(0.1)
            obj = await store.get_object(ref.id)

        assert obj is not None
        assert obj.current_stage == "index", obj.parked_reason
        assert obj.tier.value == "hot"
        assert obj.ladder_rung == 5
        assert "AETHER" in result.text.upper()
        pool = await store.get_pool()
        assert (
            await pool.fetchval(
                "SELECT count(*) FROM ingest_source_events WHERE object_id=$1 AND ladder_rung=5",
                ref.id,
            )
            == 1
        )
    finally:
        pool = await store.get_pool()
        await pool.execute("DELETE FROM brain_objects WHERE id=$1", ref.id)
        await store.close_pool()
