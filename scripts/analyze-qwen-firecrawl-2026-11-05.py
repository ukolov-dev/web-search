#!/usr/bin/env python3
"""Recount paired selector outcomes from frozen captures; no services or inference."""
import argparse
import hashlib
import json
import runpy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
helpers = runpy.run_path(str(ROOT / "scripts/analyze-firecrawl-2026-11-05.py"))
load, fold, score, latency = (helpers[k] for k in ("load", "fold", "score", "latency"))


def sha(data):
    return hashlib.sha256(data).hexdigest()


def analyze(run):
    cases, annotations = load(run / "cases.json"), load(run / "gold.json")
    pages = {p["page_id"]: p for p in cases["pages"]}
    gold = {p["page_id"]: p for p in annotations["rows"]}
    assert len(pages) == len(gold) == 12
    grades = {p: g["grade"] for p, g in gold.items()}
    assert list(grades.values()).count(2) == 8
    results, groups, rows = {a: [] for a in ("baseline", "fixed")}, {}, []
    for group in cases["groups"]:
        gid = group["case"]["id"]
        selected = {pid: p for pid, p in pages.items() if p["group"] == gid}
        local = {p["id"]: pid for pid, p in selected.items()}
        groups[gid] = {}
        for arm in results:
            data = load(run / "evidence-capture" / f"{gid}-{arm}" / "result.json")
            assert data["reader_arm"] == "firecrawl" and not data["errors"]
            assert set(data["labels"]) == local.keys()
            assert len(data["model_calls"]) <= 2 and len(data["reranker_calls"]) <= 3
            assert all(c["status"] == "ok" for c in data["reranker_calls"])
            direct = {local[i] for i, label in data["labels"].items() if label == 0}
            per = score({pid: grades[pid] for pid in selected}, direct)
            groups[gid][arm] = {**per, "direct_ids": sorted(direct),
                "model_calls": len(data["model_calls"]), "qwen_calls": len(data["reranker_calls"]),
                "qwen_pairs": sum(len(c["trace"]["pool_ranges"]) for c in data["reranker_calls"]),
                "verification_stage_ms": data["verification_elapsed_ms"],
                "qwen_elapsed_ms": [c["trace"]["call"]["elapsed_ms"] for c in data["reranker_calls"]],
                "generation_elapsed_ms": [c["elapsed_ms"] for c in data["model_calls"]]}
            results[arm].append((gid, data))
    for pid, page in pages.items():
        doc = load(run / "reader-capture" / f"{pid}-r0-firecrawl.document.json")
        assert sha(doc["text"].encode()) == doc["text_hash"]
        body = fold(doc["text"])
        raw = body.encode()
        row = {"page_id": pid, "group": page["group"], "url": page["url"], "grade": grades[pid],
            "body_chars": len(doc["text"]), "witnesses": gold[pid]["witnesses"],
            "body_witnesses": [fold(q) in body for q in gold[pid]["witnesses"]],
            "date_applicable": gold[pid]["date_applicable"],
            "expected_date": gold[pid]["expected_publication_date"],
            "parsed_date": doc["news"].get("published_date"),
            "own_date_preserved": (doc["news"].get("kind") == "article" and doc["news"].get("date_status") == "valid" and doc["news"].get("published_date") == gold[pid]["expected_publication_date"]) if gold[pid]["date_applicable"] else None,
            "arms": {}}
        scores = {}
        for arm in results:
            data = next(d for gid, d in results[arm] if gid == page["group"])
            record = next(d for d in data["documents"] if d["id"] == page["id"])
            entry = next((e for e in data["memory"]["entries"] if e["id"] == page["id"]), None)
            passages = [p for p in entry["passages"] if p["field"] == "body"] if entry else []
            assert len(passages) <= 3
            context = json.loads(next(m["content"] for m in data["model_calls"][0]["input"]["messages"] if m["role"] == "user"))
            candidate = next((p for p in context["candidates"] if p["id"] == page["id"]), None)
            texts = []
            for passage in passages:
                text = raw[passage["start"]:passage["end"]].decode("utf-8")
                assert text and len(text) <= 1200 and passage["normalized_text_hash"] == sha(raw)
                assert next(p["text"] for p in candidate["passages"] if p["id"] == passage["id"]) == text
                texts.append(text)
            call = next((c for c in data["reranker_calls"] if c["id"] == page["id"]), None)
            if call:
                trace = call["trace"]
                assert len(trace["pool_ranges"]) == len(trace["call"]["response"]["results"])
                scores[arm] = sorted((v["index"], v["relevance_score"]) for v in trace["call"]["response"]["results"])
            row["arms"][arm] = {"direct": data["labels"][page["id"]] == 0,
                "date_check": record["news"]["date_check"],
                "reranked": call is not None, "selector": call["trace"]["selector"] if call else None,
                "selected_ranges": [[p["start"], p["end"]] for p in passages],
                "selected_witnesses": [any(fold(q) in text for text in texts) for q in gold[pid]["witnesses"]],
                "check_statuses": [c["status"] for c in entry["checks"]] if entry else [],
                "audit_supported": entry["audit_supported"] if entry else None}
        row["qwen_score_vectors_identical"] = scores.get("baseline") == scores.get("fixed") if scores else None
        rows.append(row)
    arms = {}
    for arm in results:
        direct = {r["page_id"] for r in rows if r["arms"][arm]["direct"]}
        reranked = [r for r in rows if r["arms"][arm]["reranked"]]
        arms[arm] = {"quality": score(grades, direct),
            "reranked_pages": len(reranked),
            "body_witnesses_present": sum(sum(r["body_witnesses"]) for r in rows),
            "body_witnesses_total": sum(len(r["witnesses"]) for r in rows),
            "selected_witnesses_retained": sum(sum(r["arms"][arm]["selected_witnesses"]) for r in reranked),
            "selected_witnesses_eligible_total": sum(len(r["witnesses"]) for r in reranked),
            "positive_selected_witnesses_retained": sum(sum(r["arms"][arm]["selected_witnesses"]) for r in reranked if r["grade"] == 2),
            "positive_selected_witnesses_eligible_total": sum(len(r["witnesses"]) for r in reranked if r["grade"] == 2),
            "positive_witnesses_total": sum(len(r["witnesses"]) for r in rows if r["grade"] == 2),
            "model_calls": sum(g[arm]["model_calls"] for g in groups.values()),
            "qwen_calls": sum(g[arm]["qwen_calls"] for g in groups.values()),
            "qwen_pairs": sum(g[arm]["qwen_pairs"] for g in groups.values()),
            "verification_stage_latency": latency([g[arm]["verification_stage_ms"] * 1000 for g in groups.values()]),
            "qwen_call_latency": latency([ms * 1000 for g in groups.values() for ms in g[arm]["qwen_elapsed_ms"]]),
            "generation_call_latency": latency([ms * 1000 for g in groups.values() for ms in g[arm]["generation_elapsed_ms"]])}
    timings = load(run / "reader-capture/timings.json")["rows"]
    assert len(timings) == 72
    return {"scope": cases["scope"], "cohort_complete": True, "pages": 12,
        "positive_pages": 8, "negative_background_pages": 4, "gold_sha256": sha((run / "gold.json").read_bytes()),
        "arms": arms, "per_group": groups, "per_page": rows,
        "dates": {"preserved": sum(r["own_date_preserved"] is True for r in rows), "applicable": sum(r["date_applicable"] for r in rows)},
        "shared_reader_latency": latency([t["elapsed_us"] for t in timings if t["arm"] == "firecrawl"]),
        "reader_errors": [t for t in timings if t["error"]],
        "gained_tp_ids": sorted(set(arms["fixed"]["quality"]["tp_ids"]) - set(arms["baseline"]["quality"]["tp_ids"])),
        "lost_tp_ids": sorted(set(arms["baseline"]["quality"]["tp_ids"]) - set(arms["fixed"]["quality"]["tp_ids"])),
        "new_false_direct_ids": sorted(set(arms["fixed"]["quality"]["fp_ids"]) - set(arms["baseline"]["quality"]["fp_ids"])),
        "limits": ["Historical prior-reviewed pages, new URLs only relative to previous Firecrawl12, not fresh web holdout.", "Same own metadata/rawHtml and Firecrawl r0 text in both arms; no initial classifier/retrieval.", "One paired generative pass per group; model latency and final labels have variability.", "Reader latency measured separately; verification walltime loads cached reader documents. No CPU-only speed claim."]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", type=Path, default=ROOT / "runs/qwen-firecrawl-2026-11-05")
    parser.add_argument("--output", type=Path, default=ROOT / "feasibility/qwen-firecrawl-results-2026-11-05.json")
    args = parser.parse_args()
    result = analyze(args.run_dir.resolve())
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"arms": result["arms"], "gained": result["gained_tp_ids"], "lost": result["lost_tp_ids"], "new_fp": result["new_false_direct_ids"]}, ensure_ascii=False))
