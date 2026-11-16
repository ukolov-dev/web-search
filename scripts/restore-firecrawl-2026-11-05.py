#!/usr/bin/env python3
"""Restore the archived component study into a new directory; never execute its files."""
import base64
import argparse
import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def child(root, name):
    path = Path(name)
    assert not path.is_absolute() and ".." not in path.parts, "unsafe archive path"
    result = (root / path).resolve()
    assert result.is_relative_to(root.resolve()), "archive path outside output"
    return result


assert child(Path("/tmp/example"), "inputs/P01.html") == Path("/tmp/example/inputs/P01.html").resolve()
try:
    child(Path("/tmp/example"), "../escape")
except AssertionError:
    pass
else:
    raise AssertionError("unsafe archive path accepted")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--corpus", type=Path, default=ROOT / "feasibility/firecrawl-corpus-2026-11-05.json")
    args = parser.parse_args()
    out = args.output.resolve()
    assert not out.exists(), "Never overwrite an earlier study"
    corpus = json.loads(args.corpus.read_text())
    assert corpus["schema"] == "firecrawl-component-artifact-bundle-v1"
    blobs = {}
    for identifier, blob in corpus["blobs"].items():
        assert blob["encoding"] == "gzip+base64"
        data = gzip.decompress(base64.b64decode(blob["data"], validate=True))
        assert hashlib.sha256(data).hexdigest() == identifier
        blobs[identifier] = data
    for name, identifier in corpus["files"].items():
        path = child(out, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(blobs[identifier])
    print(f"Restored {len(corpus['files'])} verified artifacts into {out}; no services or models started")
