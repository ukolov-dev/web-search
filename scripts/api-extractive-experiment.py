#!/usr/bin/env python3
"""Test-only arm B: one frozen witness -> its original complete sentence.

JSON stdin: {text, start, end}; caller verifies the frozen witness equals text[start:end].
Offsets are Unicode code points in the full original source, never chunk offsets.
No question, gold, candidate ranking or network inputs. Output is experimental;
exact provenance is not proof that a sentence answers a question or is true.
Caller must supply the full source (a chunk may begin mid-sentence), and verify
its identity/version and frozen quote before calling. Original-source language
is preserved: English source yields English output, not a finished Russian UX.
Punctuation plus conservative abbreviation rejection is a bounded heuristic,
not a linguistic completeness guarantee. No source truth/time validation here.
Only whitespace is collapsed in claim; quote is always the original slice.
"""
import json
import re
import sys
import unittest


def extract(text, start, end):
    def withhold(reason):
        return {"status": "withheld", "reason": reason, "claim": None,
                "quote": None, "start": None, "end": None}

    source = text
    if not isinstance(source, str):
        return withhold("invalid_input")
    if (type(start) is not int or type(end) is not int
            or not 0 <= start < end <= len(source)):
        return withhold("invalid_witness_range")

    # Same punctuation boundary as research.rs sentence_ranges. Do not reset
    # at blank lines: that could discard a preceding condition without a stop.
    ranges = []
    sentence_start = 0
    for match in re.finditer(r"[.!?](?=\s|¶(?:\s|$)|$)", source):
        sentence_end = match.end()
        while sentence_start < sentence_end and source[sentence_start].isspace():
            sentence_start += 1
        ranges.append((sentence_start, sentence_end))
        sentence_start = sentence_end + int(source[sentence_end:sentence_end + 1] == "¶")
    containing = [(a, b) for a, b in ranges if a <= start < end <= b]
    if len(containing) != 1:
        return withhold("no_complete_containing_sentence")
    a, b = containing[0]
    # Abbreviation/initial/list-number boundaries are not confidently sentence
    # ends. Fail closed on the selected span and its immediately prior boundary.
    prior = source[:a].rstrip()
    boundary_context = prior[-40:] + " " + source[a:b]
    if re.search(r"\b(?:[A-Za-z]|[0-9]+|Mr|Mrs|Ms|Dr|Prof|vs|etc|e\.g|i\.e)\.(?:\s|$)", boundary_context):
        return withhold("ambiguous_sentence_boundary")
    sentence = source[a:b]
    if len(sentence) > 360:
        return withhold("complete_sentence_exceeds_quote_limit")
    claim = " ".join(sentence.split())
    if len(claim) > 240:
        return withhold("complete_sentence_exceeds_claim_limit")
    if len(sentence) < 12:
        return withhold("sentence_below_quote_minimum")
    if any((ord(c) < 32 or 127 <= ord(c) <= 159) and c not in "\n\r\t" for c in sentence):
        return withhold("sentence_has_control_characters")
    return {"status": "ok", "reason": "complete_original_sentence",
            "claim": claim, "quote": sentence, "start": a, "end": b}


class Checks(unittest.TestCase):
    def run_extract(self, text, quote):
        start = text.index(quote)
        return extract(text, start, start + len(quote))

    def test_preserves_condition_and_unicode(self):
        text = "Introductory sentence. Если флаг включён, load() returns bytes. Other sentence."
        result = self.run_extract(text, "load() returns bytes")
        self.assertEqual(result["claim"], "Если флаг включён, load() returns bytes.")
        span = result
        self.assertEqual(text[span["start"]:span["end"]], result["claim"])

    def test_limits_no_clipping(self):
        for length, reason in [(240, None), (241, "complete_sentence_exceeds_claim_limit"),
                               (360, "complete_sentence_exceeds_claim_limit"),
                               (361, "complete_sentence_exceeds_quote_limit")]:
            text = "When enabled, " + "x" * (length - 15) + "."
            result = self.run_extract(text, "enabled")
            self.assertEqual(result["reason"], reason or "complete_original_sentence")
            if reason:
                self.assertIsNone(result["claim"])
                self.assertIsNone(result["quote"])

    def test_rejects_invalid_range(self):
        for start, end in [(-1, 4), (0, 100), (5, 2), (True, 8)]:
            self.assertEqual(extract("The API returns bytes.", start, end)["reason"], "invalid_witness_range")

    def test_incomplete_and_cross_sentence(self):
        for text, quote in [("If enabled, returns bytes", "returns"),
                            ("Returns bytes. Writes text.", "bytes. Writes")]:
            self.assertEqual(self.run_extract(text, quote)["reason"],
                             "no_complete_containing_sentence")

    def test_no_semantic_slot_selection(self):
        text = "API returns bytes. API writes text."
        self.assertEqual(self.run_extract(text, "writes")["claim"], "API writes text.")
        self.assertEqual(self.run_extract(text, "returns")["claim"], "API returns bytes.")

    def test_ambiguous_abbreviation_withheld(self):
        self.assertEqual(self.run_extract("Use e.g. bytes when enabled.", "bytes")["status"], "withheld")

    def test_wrapped_condition_preserved(self):
        result = self.run_extract("If enabled\n\nAPI returns bytes.", "returns")
        self.assertEqual(result["claim"], "If enabled API returns bytes.")
        self.assertEqual(result["quote"], "If enabled\n\nAPI returns bytes.")

    def test_exact_repeated_witness_offset(self):
        text = "If enabled, returns bytes. Otherwise returns text."
        start = text.rindex("returns")
        self.assertEqual(extract(text, start, start + 7)["claim"], "Otherwise returns text.")

    def test_quote_raw_limit_before_normalization(self):
        text = "If enabled," + " " * 350 + " bytes."
        self.assertEqual(self.run_extract(text, "bytes")["reason"], "complete_sentence_exceeds_quote_limit")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        record = json.load(sys.stdin)
        print(json.dumps(extract(record["text"], record["start"], record["end"]), ensure_ascii=False))
