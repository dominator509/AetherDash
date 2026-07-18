"""CPU-first OCR and optional GPU acceleration."""

from server.ingest.ocr.pipeline import (
    CpuRapidOcrEngine,
    OcrPipeline,
    OcrResult,
    build_ocr_engine,
)

__all__ = ["CpuRapidOcrEngine", "OcrPipeline", "OcrResult", "build_ocr_engine"]
