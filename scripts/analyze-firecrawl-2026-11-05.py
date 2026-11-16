#!/usr/bin/env python3
"""Score the frozen source-reader component study; no network or model calls."""
import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "runs/firecrawl-2026-11-05"


def load(path):
    return json.loads(path.read_text())


def fold(text):
    return " ".join(text.split())


def score(grades, direct):
    positives = {p for p, g in grades.items() if g == 2}
    return {"tp_ids": sorted(direct & positives), "fn_ids": sorted(positives - direct),
            "fp_ids": sorted(p for p in direct if grades[p] in (0, 1)),
            "unknown_direct_ids": sorted(p for p in direct if grades[p] is None),
            "direct_recall": len(direct & positives) / len(positives) if positives else None}


def latency(values):
    if not values:
        return None
    values = sorted(values)
    return {"n": len(values), "min_ms": values[0] / 1000,
            "p50_ms": statistics.median(values) / 1000,
            "p95_ms": values[math.ceil(.95 * len(values))-1] / 1000,
            "max_ms": values[-1] / 1000}


assert score({"a": 2, "b": 1, "c": None}, {"b", "c"}) == {
    "tp_ids": [], "fn_ids": ["a"], "fp_ids": ["b"], "unknown_direct_ids": ["c"], "direct_recall": 0}
assert score({}, set())["direct_recall"] is None
assert latency([1000, 3000])["p50_ms"] == 2


def run(latency_dir):
    cases, source_gold = load(RUN / "cases.json"), load(RUN / "gold.json")
    pages = {p["page_id"]: p for p in cases["pages"]}
    gold = {p["page_id"]: p for p in source_gold["rows"]}
    assert pages.keys() == gold.keys() and len(pages) == 12
    results, preservation, quality = {}, [], {}
    times = load(RUN / latency_dir / "timings.json")["rows"]
    assert len(times) == 72 and len({(r["page_id"], r["round"], r["arm"]) for r in times}) == 72
    for pid, page in pages.items():
        for arm in ("baseline", "firecrawl"):
            docs = []
            for round_ in range(3):
                path = RUN / "reader-capture" / f"{pid}-r{round_}-{arm}.document.json"
                if path.exists():
                    d = load(path)
                    assert d["text_hash"] == hashlib.sha256(d["text"].encode()).hexdigest()
                    docs.append(d)
            doc = load(RUN / "reader-capture" / f"{pid}-r0-{arm}.document.json") if (RUN / "reader-capture" / f"{pid}-r0-{arm}.document.json").exists() else None
            body = fold(doc["text"]) if doc else ""
            needles = [{"quote": q, "preserved": fold(q) in body} for q in gold[pid]["witnesses"]]
            date = doc["news"].get("published_date") if doc else None
            ok_date = bool(doc and doc["news"].get("kind") == "article" and doc["news"].get("date_status") == "valid" and date == gold[pid]["expected_publication_date"]) if gold[pid]["date_applicable"] else None
            preservation.append({"page_id": pid, "arm": arm, "url": page["url"], "grade": gold[pid]["grade"],
                "reader_successes": len(docs), "document_present": doc is not None, "body_chars": len(doc["text"]) if doc else 0,
                "witnesses": needles, "all_body_witnesses_preserved": all(r["preserved"] for r in needles),
                "expected_date": gold[pid]["expected_publication_date"], "parsed_date": date, "own_date_preserved": ok_date,
                "reader_text_deterministic": len({d["text_hash"] for d in docs}) <= 1})
    for group in cases["groups"]:
        gid = group["case"]["id"]
        per = {}
        selected = {pid: p for pid, p in pages.items() if p["group"] == gid}
        id_to_page = {p["id"]: pid for pid, p in selected.items()}
        for arm in ("baseline", "firecrawl"):
            path = RUN / "evidence-capture" / f"{gid}-{arm}" / "result.json"
            if not path.exists():
                per[arm] = {"status": "not_completed"}
                continue
            result = load(path)
            assert set(result["labels"]) == id_to_page.keys()
            direct = {id_to_page[i] for i, k in result["labels"].items() if k == 0}
            graded = score({pid: gold[pid]["grade"] for pid in selected}, direct)
            per[arm] = {"status": "completed", **graded, "direct_ids": sorted(direct),
                "errors": result["errors"], "model_calls": len(result["model_calls"]),
                "reranker_calls": len(result["reranker_calls"]),
                "reranker_pairs": sum(len(r["trace"]["pool_ranges"]) for r in result["reranker_calls"] if r["status"] == "ok"),
                "reranker_fallbacks": [r for r in result["reranker_calls"] if r["status"] != "ok"],
                "verification_elapsed_ms": result["verification_elapsed_ms"]}
        quality[gid] = per
    all_paired = all(all(v.get("status") == "completed" for v in group.values()) for group in quality.values())
    for arm in ("baseline", "firecrawl"):
        rows = [r for r in preservation if r["arm"] == arm]
        direct = {p for group in quality.values() for p in group[arm].get("direct_ids", [])}
        graded = score({pid: g["grade"] for pid, g in gold.items()}, direct) if all_paired else None
        paired_pages = {pid for pid in pages if all(not t["error"] for t in times if t["page_id"] == pid)}
        results[arm] = {"documents_successful": sum(r["document_present"] for r in rows),
            "all_witnesses_pages": sum(r["all_body_witnesses_preserved"] for r in rows),
            "witnesses_preserved": sum(w["preserved"] for r in rows for w in r["witnesses"]),
            "witnesses_total": sum(len(r["witnesses"]) for r in rows),
            "own_dates_preserved": sum(r["own_date_preserved"] is True for r in rows), "applicable_dates": 6,
            "quality": graded, "latency_all_attempts": latency([r["elapsed_us"] for r in times if r["arm"] == arm]),
            "latency_common_successful_pages": latency([r["elapsed_us"] for r in times if r["arm"] == arm and r["page_id"] in paired_pages]),
            "reader_failures": [r for r in times if r["arm"] == arm and r["error"]]}
    new_fp = sorted(set(results["firecrawl"]["quality"]["fp_ids"]) - set(results["baseline"]["quality"]["fp_ids"])) if all_paired else None
    return {"scope": "Historical fixed12source component; defaultprovider main-content cleanup vs currentRust. No retrieval, initial classifier, publicJS or live target reads.",
        "cohort_complete": all_paired, "gold_sha256": hashlib.sha256((RUN / "gold.json").read_bytes()).hexdigest(),
        "candidate": "Upstream v2/parse uploaded HTML -> cleaned HTML -> same Rust formatted_text; own dates from hash-matching returned rawHtml",
        "reader_repetitions": 3, "latency_directory": latency_dir, "latency_definition": "Corrected client operation elapsed excluding archival I/O in both arms, three repetitions, nearest-rank p95; original asymmetrical timings retained separately",
        "arms": results, "per_page": preservation, "per_group": quality, "new_false_direct_ids": new_fp,
        "limits": ["Seven prior-known positive sources and five negative/background controls, selected before newreaderoutcomes; not fresh holdout.",
            "Quality onepairedmodelpass pergroup; sameLLM is coverage/audit, variability not isolated from body/reranker differences.",
            "Dates use returned rawHtml ownership under Rust, not Firecrawl generic publishedTime or the cleanedtext alone.",
            "No browser/download/networkprovenance qualification; /parse uses synthetic URL mapped explicitly to original snapshot URL.",
            "Latency on successful common11pages separated from allattempts, including baselineP11 failure; infrastructure startup excluded."]}


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--latency-dir", required=True)
    p.add_argument("--run-dir", type=Path, default=RUN)
    p.add_argument("--output", default="feasibility/firecrawl-results-2026-11-05.json")
    args = p.parse_args()
    RUN = args.run_dir.resolve()
    out = run(args.latency_dir)
    (ROOT / args.output).write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"cohort_complete": out["cohort_complete"], "arms": out["arms"], "new_false_direct_ids": out["new_false_direct_ids"]}, ensure_ascii=False))
