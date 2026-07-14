"""Supervised no-network parser subprocess with hard resource ceilings."""

import ctypes
import os
import subprocess
import sys
from pathlib import Path

_MAX_BY_KIND = {
    "text": 10 * 1024 * 1024,
    "email": 10 * 1024 * 1024,
    "document": 10 * 1024 * 1024,
    "screenshot": 20 * 1024 * 1024,
}
_TIMEOUT_SECONDS = 30
_MEMORY_LIMIT = 256 * 1024 * 1024


def _windows_job(process: subprocess.Popen) -> int | None:
    """Assign the child to a kill-on-close Windows Job with CPU/RAM caps."""
    if os.name != "nt":
        return None
    from ctypes import wintypes

    class BasicLimit(ctypes.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes.c_longlong),
            ("PerJobUserTimeLimit", ctypes.c_longlong),
            ("LimitFlags", wintypes.DWORD),
            ("MinimumWorkingSetSize", ctypes.c_size_t),
            ("MaximumWorkingSetSize", ctypes.c_size_t),
            ("ActiveProcessLimit", wintypes.DWORD),
            ("Affinity", ctypes.c_size_t),
            ("PriorityClass", wintypes.DWORD),
            ("SchedulingClass", wintypes.DWORD),
        ]

    class IoCounters(ctypes.Structure):
        _fields_ = [
            (name, ctypes.c_ulonglong)
            for name in (
                "ReadOperationCount",
                "WriteOperationCount",
                "OtherOperationCount",
                "ReadTransferCount",
                "WriteTransferCount",
                "OtherTransferCount",
            )
        ]

    class ExtendedLimit(ctypes.Structure):
        _fields_ = [
            ("BasicLimitInformation", BasicLimit),
            ("IoInfo", IoCounters),
            ("ProcessMemoryLimit", ctypes.c_size_t),
            ("JobMemoryLimit", ctypes.c_size_t),
            ("PeakProcessMemoryUsed", ctypes.c_size_t),
            ("PeakJobMemoryUsed", ctypes.c_size_t),
        ]

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateJobObjectW.restype = wintypes.HANDLE
    kernel32.SetInformationJobObject.argtypes = [
        wintypes.HANDLE,
        ctypes.c_int,
        ctypes.c_void_p,
        wintypes.DWORD,
    ]
    kernel32.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    job = kernel32.CreateJobObjectW(None, None)
    if not job:
        raise OSError(ctypes.get_last_error(), "CreateJobObjectW failed")
    info = ExtendedLimit()
    info.BasicLimitInformation.LimitFlags = 0x2 | 0x100 | 0x2000
    info.BasicLimitInformation.PerProcessUserTimeLimit = 10 * 10_000_000
    info.ProcessMemoryLimit = _MEMORY_LIMIT
    if not kernel32.SetInformationJobObject(
        job, 9, ctypes.byref(info), ctypes.sizeof(info)
    ):
        kernel32.CloseHandle(job)
        raise OSError(ctypes.get_last_error(), "SetInformationJobObject failed")
    if not kernel32.AssignProcessToJobObject(job, wintypes.HANDLE(process._handle)):
        kernel32.CloseHandle(job)
        raise OSError(ctypes.get_last_error(), "AssignProcessToJobObject failed")
    return job


def parse_sandboxed(kind: str, raw_bytes: bytes) -> str:
    """Parse hostile bytes outside the service with enforced limits."""
    maximum = _MAX_BY_KIND.get(kind)
    if maximum is None:
        raise ValueError(f"Unknown content kind: {kind}")
    if len(raw_bytes) > maximum:
        raise ValueError(f"{kind} input exceeds {maximum} byte limit")
    worker = Path(__file__).with_name("worker.py")
    process = subprocess.Popen(
        [sys.executable, "-I", str(worker), kind],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={"PATH": os.environ.get("PATH", ""), "PYTHONIOENCODING": "utf-8"},
        creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
    )
    job = _windows_job(process)
    try:
        stdout, stderr = process.communicate(raw_bytes, timeout=_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate()
        raise TimeoutError(f"parser exceeded {_TIMEOUT_SECONDS}s wall limit") from None
    finally:
        if job is not None:
            ctypes.WinDLL("kernel32", use_last_error=True).CloseHandle(job)
    if process.returncode != 0:
        error_type = stderr.decode("ascii", errors="ignore").strip()
        raise ValueError(
            f"parser rejected hostile or malformed content ({error_type or 'ChildExit'})"
        )
    if len(stdout) > 2 * 1024 * 1024:
        raise ValueError("parser output exceeds 2 MiB")
    return stdout.decode("utf-8")
