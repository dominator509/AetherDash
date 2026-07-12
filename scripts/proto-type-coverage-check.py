#!/usr/bin/env python3
"""Cross-language proto type-name coverage check.

NOTE: This check only verifies that type *names* from .proto files exist in
Python and TypeScript mirrors. It does NOT verify field names, types, or
wire compatibility — that is the job of the golden-vector tests.

For each service .proto file in proto/aether/ (excluding core/):
  1. Parse all message and enum type names from the .proto file.
  2. Verify each name exists in the Python PROTO_TYPE_REGISTRY.
  3. Verify each name exists as an export in the TypeScript services.ts.
  4. Verify the Rust crate compiles (handled by caller).
"""

import os
import re
import sys
from pathlib import Path

# Walk up from this script to find the project root
ROOT = Path(__file__).resolve().parent.parent

PROTO_DIR = ROOT / "proto" / "aether"
PY_PROTO = ROOT / "pylib" / "aether_py" / "proto" / "__init__.py"
TS_SERVICES = ROOT / "packages" / "types" / "src" / "proto" / "services.ts"


def extract_proto_types(proto_dir: Path) -> dict[str, set[str]]:
    """Return {package: {type_name, ...}} for all non-core proto files."""
    result: dict[str, set[str]] = {}
    for proto_file in sorted(proto_dir.rglob("*.proto")):
        rel = str(proto_file.relative_to(proto_dir))
        if rel.startswith("core" + os.sep):
            continue  # skip core types (tested by golden tests)
        pkg = ""
        types: set[str] = set()
        content = proto_file.read_text(encoding="utf-8")
        for line in content.splitlines():
            stripped = line.strip()
            if stripped.startswith("package ") and ";" in stripped:
                pkg = stripped.split()[1].rstrip(";")
            m = re.match(r"^message\s+(\w+)\s*\{", stripped)
            if m:
                types.add(m.group(1))
            m = re.match(r"^enum\s+(\w+)\s*\{", stripped)
            if m:
                types.add(m.group(1))
        if types:
            result[pkg] = types
    return result


def parse_python_registry(py_path: Path) -> set[str]:
    """Return set of type names in PROTO_TYPE_REGISTRY (BaseModel types)."""
    types: set[str] = set()
    content = py_path.read_text(encoding="utf-8")
    # Extract only the PROTO_TYPE_REGISTRY dict body
    m = re.search(r"PROTO_TYPE_REGISTRY.*?=\s*\{(.*?)\}", content, re.DOTALL)
    if not m:
        print("ERROR: PROTO_TYPE_REGISTRY not found")
        return types
    dict_body = m.group(1)
    # Find ALL "Name": Name pairs
    for m2 in re.finditer(r'"(\w+)"\s*:\s*\w+', dict_body):
        types.add(m2.group(1))
    return types


def parse_python_enums(py_path: Path) -> set[str]:
    """Return set of enum class names defined in the proto mirror module."""
    enums: set[str] = set()
    content = py_path.read_text(encoding="utf-8")
    for m in re.finditer(r"class\s+(\w+)\(StrEnum\):", content):
        enums.add(m.group(1))
    return enums


def parse_typescript_exports(ts_path: Path) -> set[str]:
    """Return set of exported interface/type names from services.ts."""
    types: set[str] = set()
    content = ts_path.read_text(encoding="utf-8")
    for line in content.splitlines():
        m = re.match(r"^export (?:interface|type)\s+(\w+)", line.strip())
        if m:
            types.add(m.group(1))
    return types


def main() -> int:
    errors = 0
    proto_types = extract_proto_types(PROTO_DIR)
    py_types = parse_python_registry(PY_PROTO)
    py_enums = parse_python_enums(PY_PROTO)
    ts_types = parse_typescript_exports(TS_SERVICES)

    print(f"Proto packages (non-core): {len(proto_types)}")
    for pkg in sorted(proto_types):
        print(f"  {pkg}: {len(proto_types[pkg])} types")

    for pkg, msgs in sorted(proto_types.items()):
        for msg in sorted(msgs):
            # Python: check registry for BaseModel types, enum definitions for enums
            py_ok = msg in py_types or msg in py_enums
            if not py_ok:
                print(f"  MISSING (Python): {pkg}.{msg}")
                errors += 1
            # TypeScript: check exported interfaces/types
            if msg not in ts_types:
                print(f"  MISSING (TypeScript): {pkg}.{msg}")
                errors += 1

    if errors:
        print(f"\nFAIL: {errors} type(s) missing from language mirrors")
        return 1

    total_types = sum(len(v) for v in proto_types.values())
    print(
        f"\ncross-language type-name coverage check: ok ({total_types} service types across {len(proto_types)} packages)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
