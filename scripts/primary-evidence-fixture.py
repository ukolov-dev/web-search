#!/usr/bin/env python3
"""Offline full-audit Session fixture restoration; no model, network or gold input.

restore(capturepath) reads the last numbered audit capture, document snapshots and
result source records. Ambiguous chunks/own quotes fail closed (no selected_start
heuristic). context_text is the untouched captured model input; context is the
Session fixture with restored chunk offsets and stripped Quote evidence. Root
must run prepare and freeze its new context before comparing augmented arms.
"""
import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def version(document):
    value = ["research-extract-v1", document["url"],
             digest(document["text"].encode()), document["metadata"]]
    return digest(json.dumps(value, ensure_ascii=False, sort_keys=True,
                             separators=(",", ":")).encode())


def unique_start(text, part, label):
    start = text.find(part)
    require(part and start >= 0 and text.find(part, start + 1) < 0,
            f"nonunique/missing original slice: {label}")
    return start


def restore(capturepath):
    directory = Path(capturepath)
    audits = sorted(directory.glob("model-*-audit.json"),
                    key=lambda p: int(p.name.split("-")[1]))
    require(bool(audits), "no audit capture")
    audit_path = audits[-1]
    capture = load(audit_path)
    original = capture["input"]["messages"][-1]["content"]
    context = json.loads(original)
    source_records = load(directory / "result.json")["sources"]
    sources = {s["id"]: s for s in source_records}
    require(len(sources) == len(source_records), "duplicate source ids")
    docs = [(p, load(p)) for p in sorted(directory.glob("document-*.json"))]
    provenance = []
    bodies = {}
    for source in context["sources"]:
        record = sources[source["id"]]
        matches = [(p, d) for p, d in docs if d["url"] == record["url"]]
        require(len(matches) == 1, f"nonunique/missing source document: {source['id']}")
        path, document = matches[0]
        text_hash = digest(document["text"].encode())
        require(text_hash == record["text_hash"], "source text SHA mismatch")
        require(version(document) == record["document_version"], "document version mismatch")
        for key in ["title", "publication", "eligible"]:
            require(source[key] == record[key], f"source metadata mismatch: {key}")
        require(source["url_preview"] == record["url"], "URL preview is truncated/changed")
        source["url_preview"] = record["url"]
        bodies[source["id"]] = document["text"]
        provenance.append({"source_id": source["id"], "document_file": path.name,
                           "file_sha256": digest(path.read_bytes()),
                           "text_sha256": text_hash, "document_version": record["document_version"]})

    chunks = copy.deepcopy(context["rejection_context"])
    index = {c["id"]: c for c in chunks}
    require(len(index) == len(chunks), "duplicate chunk ids")
    for chunk in chunks:
        record = sources[chunk["source_id"]]
        require(chunk["document_version"] == record["document_version"], "chunk version mismatch")
        require(chunk["field"] == "text", "unsupported source field")
        start = unique_start(bodies[chunk["source_id"]], chunk["text"], chunk["id"])
        chunk.update(start=start, end=start + len(chunk["text"]))
    context["chunks"] = chunks
    for row in context["claims"]:
        for quote in row["claim"]["evidence"]:
            chunk = index[quote["chunk_id"]]
            require(quote["source_id"] == chunk["source_id"]
                    and quote["document_version"] == chunk["document_version"]
                    and quote["field"] == chunk["field"], "quote source/version/field mismatch")
            start, end = quote["start"], quote["end"]
            require(type(start) is int and type(end) is int
                    and chunk["start"] <= start < end <= chunk["end"], "quote outside own chunk")
            body = bodies[chunk["source_id"]]
            require(body[start:end] == quote["quote"], "quote original offsets mismatch")
            local = unique_start(chunk["text"], quote["quote"], row["id"])
            require(chunk["start"] + local == start, "quote own-chunk offset mismatch")
            row_quote = {"chunk_id": quote["chunk_id"], "quote": quote["quote"]}
            quote.clear()
            quote.update(row_quote)
    return {"context": context, "context_text": original,
            "provenance": {"audit_file": audit_path.name,
                           "audit_sha256": digest(audit_path.read_bytes()),
                           "result_sha256": digest((directory / "result.json").read_bytes()),
                           "documents": provenance}}


class SelfCheck(unittest.TestCase):
    def test_roundtrip_and_tamper_guards(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            doc = {"url": "https://example.org/api", "text": "Пролог. API returns bytes. Tail.", "metadata": {}}
            source = {"id": "S1", "url": doc["url"], "title": "API", "eligible": True,
                      "publication": {}, "text_hash": digest(doc["text"].encode()), "document_version": version(doc)}
            chunk = {"id": "S1P1", "source_id": "S1", "document_version": version(doc),
                     "field": "text", "text": "API returns bytes."}
            quote = {"chunk_id": "S1P1", "source_id": "S1", "document_version": version(doc),
                     "field": "text", "quote": "returns bytes", "start": 12, "end": 25}
            context = {"sources": [{"id": "S1", "url_preview": doc["url"], "title": "API",
                                    "eligible": True, "publication": {}}],
                       "rejection_context": [chunk], "claims": [{"id": "A1", "claim": {"text": "API returns bytes.", "evidence": [quote]}}]}
            def save(name, value):
                (root / name).write_text(json.dumps(value, ensure_ascii=False))
            save("document-01.json", doc)
            save("result.json", {"sources": [source]})
            save("model-02-audit.json", {"input": {"messages": [{"content": json.dumps(context, ensure_ascii=False)}]}})
            fixture = restore(root)
            self.assertEqual(fixture["context"]["chunks"][0]["start"], 8)
            self.assertEqual(fixture["context"]["claims"][0]["claim"]["evidence"],
                             [{"chunk_id": "S1P1", "quote": "returns bytes"}])
            # Every gate below must fail rather than guess a different source/span.
            original_context = copy.deepcopy(context)
            for mutation, reason in [
                    (lambda q: q.update(start=11), "original offsets"),
                    (lambda q: q.update(end=100), "outside own chunk"),
                    (lambda q: q.update(document_version="wrong"), "source/version"),
                    (lambda q: q.update(quote="writes bytes"), "original offsets")]:
                altered = copy.deepcopy(original_context)
                mutation(altered["claims"][0]["claim"]["evidence"][0])
                save("model-02-audit.json", {"input": {"messages": [{"content": json.dumps(altered)}]}})
                with self.assertRaisesRegex(ValueError, reason):
                    restore(root)
            save("model-02-audit.json", {"input": {"messages": [{"content": json.dumps(original_context)}]}})
            altered_source = copy.deepcopy(source)
            altered_source["document_version"] = "wrong"
            save("result.json", {"sources": [altered_source]})
            with self.assertRaisesRegex(ValueError, "document version"):
                restore(root)
            save("result.json", {"sources": [source]})
            doc["text"] += "Changed"
            save("document-01.json", doc)
            with self.assertRaisesRegex(ValueError, "SHA"):
                restore(root)

    def test_ambiguity_rejected(self):
        for text, part in [("repeat repeat", "repeat"), ("abc", "missing"), ("abc", "")]:
            with self.assertRaises(ValueError):
                unique_start(text, part, "synthetic")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        print(json.dumps(restore(sys.argv[1]), ensure_ascii=False, indent=2))
