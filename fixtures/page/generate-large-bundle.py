#!/usr/bin/env python3
"""Regenerate fixtures/page/large-bundle.js (multi-MB Debugger.getScriptSource input)."""
from __future__ import annotations

import argparse
from pathlib import Path

DEFAULT_BYTES = 2_500_000


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--bytes",
        type=int,
        default=DEFAULT_BYTES,
        help=f"minimum payload size in bytes (default {DEFAULT_BYTES})",
    )
    p.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).with_name("large-bundle.js"),
    )
    args = p.parse_args()

    chunks: list[str] = [
        "// large-bundle.js — multi-MB fixture asset for getScriptSource benches\n",
        "// Regenerate: python3 fixtures/page/generate-large-bundle.py\n",
        "function largeBundleMarker() { return 'large-bundle'; }\n",
        "var largeBundleData = [\n",
    ]
    size = sum(len(c) for c in chunks)
    i = 0
    line = "  '%s',\n"
    while size < args.bytes:
        piece = line % (("blk%06d-" % i) + ("x" * 80))
        chunks.append(piece)
        size += len(piece)
        i += 1
    chunks.append("];\n")
    chunks.append("console.log('large-bundle loaded', largeBundleData.length);\n")
    args.out.write_text("".join(chunks), encoding="utf-8")
    print(f"wrote {args.out} ({args.out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
