#!/usr/bin/env python3
"""Offline candidate A: expand an original witness; never judge or rewrite its claim.

Offsets count Unicode code points, end exclusive, in the supplied original text.
JSON stdin: {"text": ..., "start": ..., "end": ...}; JSON stdout: candidate.
This conservative sentence heuristic is not a language parser or semantic verifier.
Abbreviations, nested syntax and cross-paragraph semantic references are unresolved.
"""
import argparse
import json
import re
import sys

QUOTE_LIMIT = 360
SOURCE_LIMIT = 100_000


def expand(text, start, end):
    """Return a bounded contiguous original slice, or withhold without truncation."""
    def withheld(reason):
        return {"status": "withhold", "reason": reason}

    if not isinstance(text, str) or len(text) > SOURCE_LIMIT:
        return withheld("invalid_source")
    if (type(start) is not int or type(end) is not int
            or not 0 <= start < end <= len(text)):
        return withheld("invalid_range")
    if any(ord(c) < 32 and c not in "\n\r\t" for c in text):
        return withheld("invalid_control")

    # Blank lines delimit available blocks, as in research.rs sentence_ranges.
    blocks = []
    block_start = 0
    for separator in re.finditer(r"\n[ \t\r]*\n", text):
        blocks.append((block_start, separator.start()))
        block_start = separator.end()
    blocks.append((block_start, len(text)))
    sentences = []
    for left, right in blocks:
        cursor = left
        for match in re.finditer(r"[.!?](?=\s|$)", text[left:right]):
            stop = left + match.end()
            while cursor < stop and text[cursor].isspace():
                cursor += 1
            if cursor < stop:
                sentences.append((cursor, stop, left, right))
            cursor = stop
    selected = [s for s in sentences if s[0] < end and start < s[1]]
    if not selected:
        return withheld("sentence_boundary_unresolved")
    left, right = selected[0][0], selected[-1][1]
    if left > start or right < end:
        return withheld("sentence_boundary_unresolved")
    # Never silently skip an unfinished paragraph between two selected sentences.
    if any(text[a[1]:b[0]].strip()
           for a, b in zip(selected, selected[1:])):
        return withheld("sentence_boundary_unresolved")

    intro = False
    block_index = next(i for i, block in enumerate(blocks) if block[0] == selected[0][2])
    if block_index and not text[selected[0][2]:left].strip():
        prior_left, prior_right = blocks[block_index - 1]
        prior = text[prior_left:prior_right].rstrip()
        if prior.endswith(":") and any(c.isalnum() for c in prior):
            left = prior_left
            while left < prior_right and text[left].isspace():
                left += 1
            intro = True

    if right - left > QUOTE_LIMIT:
        return withheld("context_over_quote_budget")
    quote = text[left:right]
    # An explicit dependent opening cannot acquire its referent from outside the slice.
    # This is an abstention hint, not an exhaustive detector or entailment decision.
    if not intro and re.match(r"(?:this|that|these|those|it|they|its|their|это|этот|эта|эти|он|она|они)\b",
                               quote, re.IGNORECASE):
        return withheld("context_unresolved")
    if len(quote) < 12:
        return withheld("below_quote_minimum")
    return {
        "status": "unchanged" if (left, right) == (start, end) else "expanded",
        "start": left, "end": right, "quote": quote,
        "offset_unit": "Unicode code points; end exclusive",
        "intro_included": intro, "semantic_verification": "not_performed",
    }


def self_check():
    checks = []

    def candidate(name, text, needle, expected):
        start = text.index(needle)
        result = expand(text, start, start + len(needle))
        assert result.get("quote") == expected, (name, result)
        assert result["start"] <= start < start + len(needle) <= result["end"]
        assert text[result["start"]:result["end"]] == result["quote"]
        byte_start = len(text[:result["start"]].encode("utf-8"))
        byte_end = len(text[:result["end"]].encode("utf-8"))
        assert text.encode("utf-8")[byte_start:byte_end] == result["quote"].encode("utf-8")
        assert len(result["quote"]) <= QUOTE_LIMIT
        assert result["semantic_verification"] == "not_performed"
        checks.append(name)

    candidate("condition_and_negation", "It worked before. Returns a value only if the key exists; otherwise it does not return one.",
              "Returns a value", "Returns a value only if the key exists; otherwise it does not return one.")
    candidate("python_versions_and_decimal", "Python 3.13 differs from Python 3.16.0a2 when the value is 3.14.",
              "3.16.0a2", "Python 3.13 differs from Python 3.16.0a2 when the value is 3.14.")
    candidate("unicode_offsets", "😀 Предисловие. Возвращает е\u0308жика 🦔 только при условии успеха. Затем конец.",
              "е\u0308жика", "Возвращает е\u0308жика 🦔 только при условии успеха.")
    candidate("paragraph_start", "Previous block.\n\nReturns the result when ready.\n\nNext block.",
              "Returns", "Returns the result when ready.")
    candidate("paragraph_end", "Returns the result when ready.",
              "ready", "Returns the result when ready.")
    candidate("explicit_contiguous_intro", "For Engine 8.2.0b2, the release contract is:\n\nThis preview is not recommended for production.",
              "not recommended", "For Engine 8.2.0b2, the release contract is:\n\nThis preview is not recommended for production.")
    candidate("multiple_sentences", "First complete condition. Second complete exception.",
              "condition. Second", "First complete condition. Second complete exception.")
    for name, text, needle, reason in [
        ("budget_no_truncation", "Returns " + "x" * 360 + " only if ready.", "Returns", "context_over_quote_budget"),
        ("unfinished_source", "Returns a result only if", "result", "sentence_boundary_unresolved"),
        ("anaphora_abstention", "Old historical context. This release is ready.", "ready", "context_unresolved"),
        ("unfinished_intermediate_block", "First complete statement.\n\nA condition without terminator\n\nFinal complete statement.", "First complete statement.\n\nA condition without terminator\n\nFinal", "sentence_boundary_unresolved"),
    ]:
        start = text.index(needle)
        result = expand(text, start, start + len(needle))
        assert result == {"status": "withhold", "reason": reason}, (name, result)
        checks.append(name)
    assert expand("text", -1, 2)["reason"] == "invalid_range"
    assert expand("text", True, 2)["reason"] == "invalid_range"
    assert expand("x" * (SOURCE_LIMIT + 1), 0, 20)["reason"] == "invalid_source"
    checks.append("invalid_input_caps")
    print(json.dumps({"status": "passed", "checks": checks, "count": len(checks)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        self_check()
    else:
        data = json.load(sys.stdin)
        print(json.dumps(expand(data["text"], data["start"], data["end"]), ensure_ascii=False))


if __name__ == "__main__":
    main()
