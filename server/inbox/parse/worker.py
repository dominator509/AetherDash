"""Isolated parser child. Invoked with Python isolated mode; never import the service."""

import io
import os
import socket
import sys


def _deny_network() -> None:
    def denied(*_args, **_kwargs):
        raise PermissionError("network disabled in parser worker")

    socket.create_connection = denied  # type: ignore[assignment]
    socket.getaddrinfo = denied  # type: ignore[assignment]

    def audit(event: str, args: tuple) -> None:
        if event.startswith("socket.") or event in {
            "subprocess.Popen",
            "os.system",
            "ctypes.dlopen",
        }:
            raise PermissionError("capability disabled in parser worker")
        if event == "open" and len(args) > 1:
            mode = str(args[1])
            if any(flag in mode for flag in ("w", "a", "+", "x")):
                raise PermissionError("filesystem writes disabled in parser worker")

    sys.addaudithook(audit)


def _resource_limits() -> None:
    if os.name != "posix":
        return
    import resource

    resource.setrlimit(resource.RLIMIT_CPU, (10, 10))
    resource.setrlimit(resource.RLIMIT_AS, (256 * 1024 * 1024, 256 * 1024 * 1024))
    resource.setrlimit(resource.RLIMIT_FSIZE, (2 * 1024 * 1024, 2 * 1024 * 1024))
    resource.setrlimit(resource.RLIMIT_NOFILE, (16, 16))


def main() -> int:
    kind = sys.argv[1]
    pdf_module = None
    if kind == "document":
        import pypdf

        pdf_module = pypdf
    _deny_network()
    _resource_limits()
    raw = sys.stdin.buffer.read(20 * 1024 * 1024 + 1)
    if kind == "_network_probe":
        try:
            socket.create_connection(("127.0.0.1", 9))
        except PermissionError:
            sys.stdout.write("blocked")
            return 0
        raise RuntimeError("network guard failed")
    if kind == "text":
        text = raw.decode("utf-8")
    elif kind == "email":
        from email import policy
        from email.parser import BytesParser

        message = BytesParser(policy=policy.default).parsebytes(raw)
        body = message.get_body(preferencelist=("plain",))
        text = "\n".join(
            filter(
                None,
                (
                    f"From: {message.get('from', '')}",
                    f"Subject: {message.get('subject', '')}",
                    body.get_content() if body is not None else "",
                ),
            )
        )
    elif kind == "screenshot":
        text = f"[Image: {len(raw)} bytes, OCR pending — EP-206]"
    elif kind == "document":
        reader = pdf_module.PdfReader(io.BytesIO(raw))
        text = "\n".join(filter(None, (page.extract_text() for page in reader.pages)))
    else:
        raise ValueError("unsupported parser kind")
    encoded = text.encode("utf-8")
    if len(encoded) > 2 * 1024 * 1024:
        raise ValueError("parser output exceeds 2 MiB")
    sys.stdout.buffer.write(encoded)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(type(exc).__name__, file=sys.stderr)
        raise SystemExit(2) from None
