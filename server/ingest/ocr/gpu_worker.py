"""Optional TensorRT OCR engine, imported only when GPU mode is requested."""

from rapidocr import EngineType, RapidOCR

from server.ingest.ocr.pipeline import CpuRapidOcrEngine


class GpuRapidOcrEngine(CpuRapidOcrEngine):
    name = "rapidocr-tensorrt-gpu"

    def __init__(self) -> None:
        engine = RapidOCR(
            params={
                "Det.engine_type": EngineType.TENSORRT,
                "Cls.engine_type": EngineType.TENSORRT,
                "Rec.engine_type": EngineType.TENSORRT,
                "EngineConfig.tensorrt.use_fp16": False,
                "EngineConfig.tensorrt.device_id": 0,
            }
        )
        super().__init__(engine)
