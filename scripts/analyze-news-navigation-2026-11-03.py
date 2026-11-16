#!/usr/bin/env python3
"""Recount the NEW8 paired responses; source gold is optional and never inferred."""
import argparse
from collections import Counter
from datetime import date
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "runs/news-navigation-2026-11-03/benchmark"
spec = importlib.util.spec_from_file_location("news_runner", ROOT / "scripts/news-navigation-benchmark-2026-11-03.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def load(path):
    return json.loads(path.read_text())


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def blind_id(case_id, url):
    return hashlib.sha256((case_id + "\n" + runner.canonical(url)).encode()).hexdigest()[:16]


def iso_date(value):
    try:
        return date.fromisoformat(value) if isinstance(value, str) else None
    except ValueError:
        return None


def annotation_map(gold, cases):
    rows = {}
    for item in gold.get("rows", []):
        token = item["blind_id"]
        assert token not in rows, f"Duplicate annotation: {token}"
        assert item.get("grade") in (None, 0, 1, 2), f"Invalid grade: {token}"
        assert item.get("kind") in ("article", "feed", "application", "other", "unknown"), f"Invalid kind: {token}"
        assert item.get("case_id") in cases, f"Unknown/missing case ID: {token}"
        assert isinstance(item.get("reason"), str) and item["reason"].strip(), f"Missing reason: {token}"
        assert not any(key in item for key in ("classification", "audit_supported", "arm")), "Source gold must not encode model labels/arm"
        if item.get("grade") == 2 and item.get("case_id") in cases:
            case = cases[item["case_id"]]
            if case["intent"] != "explicit_application_control":
                publication = iso_date(item.get("published_date"))
                assert item["kind"] == "article" and item.get("date_verified") is True and item.get("source_reviewed") is True, f"News grade2 lacks source/date verification: {token}"
                assert publication is not None and publication <= date(2026, 11, 3), f"News grade2 has unknown/future publication date: {token}"
                assert iso_date(case["published_from"]) <= publication <= iso_date(case["published_to"]), f"Grade2 outside preregistered window: {token}"
        rows[token] = item
    return rows


def lineage(value):
    trace = value.get("news_navigation") or {}
    docs = trace.get("documents", [])
    links = trace.get("links", [])
    output = []
    for doc in docs:
        parent = doc.get("parent_url")
        if not parent:
            continue
        parent_doc = next((p for p in docs if p.get("final_url", p.get("url")) == parent), None)
        linked = any(l.get("parent_url") == parent and runner.canonical(l.get("url", "")) == runner.canonical(doc.get("url", "")) for l in links)
        article_url = doc.get("final_url", doc.get("url", ""))
        fetched = doc.get("http_status") == 200 and bool(doc.get("text_hash")) and not doc.get("error")
        feed_fetched = bool(parent_doc and parent_doc.get("http_status") == 200 and parent_doc.get("html_hash") and not parent_doc.get("error"))
        same_origin = urlsplit(parent)[:2] == urlsplit(article_url)[:2]
        meta = (doc.get("news") or {}).get("metadata", {})
        output.append({"id": doc.get("id"), "url": doc.get("url"), "final_url": article_url,
            "parent_url": parent, "parent_fetched": feed_fetched, "link_recorded": linked,
            "same_origin": same_origin, "article_fetched": fetched,
            "runtime_kind": meta.get("kind", "unknown"),
            "runtime_date_eligible": (doc.get("news") or {}).get("date_check", {}).get("eligible"),
            "complete_observed_article_lineage": bool(linked and feed_fetched and same_origin and fetched and meta.get("kind") == "article")})
    return output


def case_metrics(case, arm, capture, annotations, gold_supplied, manifest_entry=None):
    value = runner.structured(capture.get("response", {})) if capture else {}
    results = value.get("results", [])
    seen, rows = set(), []
    for rank, result in enumerate(results, 1):
        url = result.get("url", "")
        canonical = runner.canonical(url)
        token = blind_id(case["id"], url)
        gold = annotations.get(token)
        assert gold is None or gold.get("case_id") == case["id"], f"Gold case binding mismatch: {token}"
        duplicate = canonical in seen
        seen.add(canonical)
        grade = gold.get("grade") if gold else None
        news = result.get("news") or {}
        meta, date_check = news.get("metadata", {}), news.get("date_check", {})
        direct = result.get("classification") == "direct"
        if not direct:
            judgement = "not_direct"
        elif grade is None:
            judgement = "unknown_direct"
        elif grade == 2:
            judgement = "known_suitable_direct"
        else:
            judgement = "known_unsuitable_direct"
        rows.append({"arm": arm, "case_id": case["id"], "id": result.get("id"), "url": url,
            "canonical_url": canonical, "blind_id": token, "rank": rank, "duplicate_url": duplicate,
            "classification": result.get("classification"), "gold_grade": grade,
            "gold_kind": gold.get("kind") if gold else None, "gold_reason": gold.get("reason") if gold else None,
            "gold_source_reviewed": gold.get("source_reviewed") if gold else None,
            "gold_date_verified": gold.get("date_verified") if gold else None,
            "gold_published_date": gold.get("published_date") if gold else None,
            "direct_judgement": judgement, "runtime_kind": meta.get("kind", "unknown"),
            "runtime_date_status": meta.get("date_status", "missing"),
            "runtime_published_date": meta.get("published_date"),
            "runtime_date_eligible": date_check.get("eligible"), "runtime_date_reason": date_check.get("reason"),
            "parent_url": news.get("parent_url"),
            "candidate_news_direct_policy_violation": bool(arm == "fixed" and case["intent"] != "explicit_application_control" and direct and (meta.get("kind") != "article" or date_check.get("eligible") is not True))})
    unique = [r for r in rows if not r["duplicate_url"]]
    counts = Counter(r["direct_judgement"] for r in unique)
    known_direct = counts["known_suitable_direct"] + counts["known_unsuitable_direct"]
    budget = value.get("budget", {})
    limits = {"logical_search_calls": 2, "classification_calls": 3, "document_reads": 4,
              "reranker_calls": 4, "source_context_max": 24000}
    budget_violations = [key for key, maximum in limits.items() if budget.get(key, 0) > maximum]
    if value.get("elapsed_ms", 0) > 180000:
        budget_violations.append("elapsed_ms")
    trace = value.get("news_navigation") or {}
    if trace and trace.get("network_reads") != budget.get("document_reads"):
        budget_violations.append("news_trace_read_count_mismatch")
    qwen_calls = value.get("page_verification", {}).get("passage_reranker", {}).get("calls", [])
    if "reranker_calls" in budget and budget["reranker_calls"] != len(qwen_calls):
        budget_violations.append("qwen_trace_count_mismatch")
    documents = value.get("page_verification", {}).get("documents", [])
    docs_unique = {d.get("id", str(i)): d for i, d in enumerate(documents)}
    date_statuses = Counter((d.get("news") or {}).get("metadata", {}).get("date_status", "missing") for d in docs_unique.values())
    kinds = Counter((d.get("news") or {}).get("metadata", {}).get("kind", "unknown") for d in docs_unique.values())
    lineage_rows = lineage(value)
    case_gold_complete = gold_supplied and all(r["blind_id"] in annotations for r in unique)
    news_case = case["intent"] != "explicit_application_control"
    qualified = sum(r["gold_grade"] == 2 and (not news_case or r["gold_kind"] == "article") for r in unique) if gold_supplied else None
    outcome = {
        "case_id": case["id"], "query": case["query"], "arm": arm, "intent": case["intent"],
        "executed_capture": bool(capture), "tool_status": value.get("status"),
        "classifier_valid": value.get("classifier_valid"),
        "unexecuted_reason": manifest_entry.get("reason") if manifest_entry and not capture else None,
        "transport_error": capture.get("transport_error") if capture else None,
        "errors": value.get("errors", value.get("error")), "returned": len(results), "unique_returned": len(unique),
        "gold_complete_for_returned": case_gold_complete, "gold_unknown_rows": sum(r["gold_grade"] is None for r in unique),
        "qualified_concrete_news_articles": qualified if news_case else None,
        "qualified_explicit_app_intent_pages": qualified if not news_case else None,
        "direct": sum(r["classification"] == "direct" for r in unique),
        "known_suitable_direct": counts["known_suitable_direct"] if gold_supplied else None,
        "known_unsuitable_direct": counts["known_unsuitable_direct"] if gold_supplied else None,
        "unknown_direct": counts["unknown_direct"],
        "judged_direct_precision": counts["known_suitable_direct"] / known_direct if known_direct else None,
        "budget": budget, "budget_violations": budget_violations,
        "wall_ms": capture.get("wall_ms") if capture else None, "elapsed_ms": value.get("elapsed_ms"),
        "runtime_read_kind_counts": dict(kinds), "runtime_read_date_status_counts": dict(date_statuses),
        "runtime_eligible_read_articles": sum((d.get("news") or {}).get("date_check", {}).get("eligible") is True for d in docs_unique.values()),
        "qwen_fallbacks": sum(c.get("status") != "ok" for c in qwen_calls),
        "lineage": lineage_rows, "complete_observed_article_lineages": sum(x["complete_observed_article_lineage"] for x in lineage_rows),
        "runtime_eligible_observed_article_lineages": sum(x["complete_observed_article_lineage"] and x["runtime_date_eligible"] is True for x in lineage_rows),
        "source_qualified_discovered_articles": sum(r["gold_grade"] == 2 and r["gold_kind"] == "article" and bool(r["parent_url"]) for r in unique) if gold_supplied else None,
        "rows": rows,
    }
    return outcome


def recount(run, gold_path=None):
    cases = load(ROOT / "feasibility/news-navigation-cases-v1.json")["cases"]
    case_map = {c["id"]: c for c in cases}
    annotations = annotation_map(load(gold_path), case_map) if gold_path else {}
    manifest = load(run / "execution.json") if (run / "execution.json").exists() else {}
    entries = {(x["case"], x["arm"]): x for x in manifest.get("calls", [])}
    metrics = []
    for case in cases:
        for arm in ("baseline", "fixed"):
            path = run / arm / (case["id"] + ".json")
            metrics.append(case_metrics(case, arm, load(path) if path.exists() else None,
                annotations, gold_path is not None, entries.get((case["id"], arm))))
    group_summary = {}
    for group, selected in (("news_nn01_nn07", [m for m in metrics if m["intent"] != "explicit_application_control"]),
                            ("explicit_app_nn08", [m for m in metrics if m["intent"] == "explicit_application_control"])):
        group_summary[group] = {}
        for arm in ("baseline", "fixed"):
            arm_cases = [m for m in selected if m["arm"] == arm]
            rows = [row for m in arm_cases for row in m["rows"] if not row["duplicate_url"]]
            direct = [row for row in rows if row["classification"] == "direct"]
            fp = [row for row in direct if row["direct_judgement"] == "known_unsuitable_direct"]
            tp = [row for row in direct if row["direct_judgement"] == "known_suitable_direct"]
            unknown = [row for row in direct if row["direct_judgement"] == "unknown_direct"]
            group_summary[group][arm] = {
                "expected_cases": len(arm_cases), "executed_cases": sum(m["executed_capture"] for m in arm_cases),
                "returned": len(rows), "direct": len(direct), "gold_available": gold_path is not None,
                "qualified_pages": sum(row["gold_grade"] == 2 for row in rows) if gold_path else None,
                "known_suitable_direct": len(tp) if gold_path else None,
                "known_unsuitable_direct": len(fp) if gold_path else None, "unknown_direct": len(unknown),
                "judged_direct_precision": len(tp)/(len(tp)+len(fp)) if tp or fp else None,
                "known_unsuitable_direct_rows": fp, "unknown_direct_rows": unknown,
                "candidate_news_direct_policy_violations": [r for r in direct if r["candidate_news_direct_policy_violation"]],
            }
        left, right = group_summary[group]["baseline"], group_summary[group]["fixed"]
        keys = lambda rows: {(r["case_id"], r["canonical_url"]) for r in rows}
        new_fp = keys(right["known_unsuitable_direct_rows"]) - keys(left["known_unsuitable_direct_rows"])
        group_summary[group]["new_known_unsuitable_direct_rows"] = [r for r in right["known_unsuitable_direct_rows"] if (r["case_id"], r["canonical_url"]) in new_fp]
        group_summary[group]["qualified_pages_delta"] = right["qualified_pages"] - left["qualified_pages"] if gold_path else None
    budget_totals = {arm: {key: sum(m["budget"].get(key, 0) for m in metrics if m["arm"] == arm)
                    for key in ("logical_search_calls", "classification_calls", "document_reads", "reranker_calls", "reranker_pairs")} for arm in ("baseline", "fixed")}
    rows = [r for m in metrics for r in m["rows"] if not r["duplicate_url"]]
    supplied = {r["blind_id"] for r in rows}
    missing = sorted(supplied - annotations.keys()) if gold_path else sorted(supplied)
    unused = sorted(annotations.keys() - supplied)
    news = group_summary["news_nn01_nn07"]
    all_captured = sum(m["executed_capture"] for m in metrics) == 16
    quality = {
        "evaluated": bool(gold_path and not missing and all_captured),
        "all16_captures": all_captured, "gold_annotation_missing_ids": missing, "unused_gold_ids": unused,
        "new_known_news_false_direct_rows": news["new_known_unsuitable_direct_rows"],
        "candidate_news_direct_policy_violations": news["fixed"]["candidate_news_direct_policy_violations"],
        "candidate_known_app_download_news_direct_rows": [r for r in news["fixed"]["known_unsuitable_direct_rows"] if r["gold_kind"] == "application"],
        "candidate_unknown_news_direct_rows": news["fixed"]["unknown_direct_rows"],
        "budget_violations": [{"case_id": m["case_id"], "arm": m["arm"], "violations": m["budget_violations"]} for m in metrics if m["budget_violations"]],
    }
    quality["passed"] = (not quality["new_known_news_false_direct_rows"] and not quality["candidate_news_direct_policy_violations"] and not quality["candidate_known_app_download_news_direct_rows"] and not quality["candidate_unknown_news_direct_rows"] and not quality["budget_violations"]) if quality["evaluated"] else None
    return {"stage": "source_gold_joined" if gold_path else "pre_gold_runtime_recount",
        "gold_sha256": runner.sha(gold_path) if gold_path else None,
        "limits": ["Live paired sources/HTML are not frozen identical inputs", "No full-web recall measured", "Runtime metadata/date eligibility is not independent source gold", "Unknown direct is not counted as known false direct", "Explicit app intent nn08 is separate from news", "Logical source budget is not proof of successful external HTTP call count", "No full model input/usage/cost in public MCP"],
        "groups": group_summary, "budget_totals": budget_totals,
        "observed_article_lineages": {arm: sum(m["complete_observed_article_lineages"] for m in metrics if m["arm"] == arm) for arm in ("baseline", "fixed")},
        "runtime_eligible_observed_article_lineages": {arm: sum(m["runtime_eligible_observed_article_lineages"] for m in metrics if m["arm"] == arm) for arm in ("baseline", "fixed")},
        "initial_classifiers_valid": {arm: sum(m["classifier_valid"] is True for m in metrics if m["arm"] == arm) for arm in ("baseline", "fixed")},
        "live_usefulness": {"source_qualified_discovered_articles_fixed": sum(m["source_qualified_discovered_articles"] or 0 for m in metrics if m["arm"] == "fixed") if gold_path else None,
            "news_qualified_pages_delta": news["qualified_pages_delta"],
            "note": "Guard safety can pass with zero useful articles; navigation/usefulness must be assessed separately."},
        "quality_gate": quality, "cases": metrics}


def self_check():
    case = {"id": "test", "intent": "news", "query": "новости города сегодня"}
    result = {"id": "C001", "url": "https://example.org/news/city/", "classification": "direct"}
    capture = {"response": {"result": {"structuredContent": {"status": "ok", "results": [result], "budget": {"document_reads": 0}}}}}
    token = blind_id("test", result["url"])
    unknown = case_metrics(case, "fixed", capture, {}, True)
    assert unknown["unknown_direct"] == 1 and unknown["known_unsuitable_direct"] == 0
    assert unknown["rows"][0]["candidate_news_direct_policy_violation"] is True
    partial = case_metrics(case, "baseline", capture, {token: {"case_id": "test", "grade": 1, "kind": "feed", "reason": "not an article"}}, True)
    assert partial["known_unsuitable_direct"] == 1 and partial["unknown_direct"] == 0
    app = dict(case, intent="explicit_application_control")
    app_metrics = case_metrics(app, "fixed", capture, {token: {"case_id": "test", "grade": 2, "kind": "application", "reason": "explicitly requested application"}}, True)
    assert app_metrics["known_suitable_direct"] == 1 and not app_metrics["rows"][0]["candidate_news_direct_policy_violation"]
    missing = case_metrics(case, "fixed", None, {}, False, {"reason": "blocked host"})
    assert not missing["executed_capture"] and missing["unexecuted_reason"] == "blocked host" and missing["qualified_concrete_news_articles"] is None
    sample = {"news_navigation": {"links": [{"parent_url": "https://example.org/", "url": result["url"]}], "documents": [
        {"url": "https://example.org/", "final_url": "https://example.org/", "http_status": 200, "html_hash": "feed"},
        {"id": "C001", "url": result["url"], "parent_url": "https://example.org/", "http_status": 200, "text_hash": "article", "news": {"metadata": {"kind": "article"}}}]}}
    assert lineage(sample)[0]["complete_observed_article_lineage"] is True
    sample["news_navigation"]["links"] = []
    assert lineage(sample)[0]["complete_observed_article_lineage"] is False
    with tempfile.TemporaryDirectory() as directory:
        counts = recount(Path(directory))
        assert len(counts["cases"]) == 16 and counts["quality_gate"]["passed"] is None
        assert counts["groups"]["news_nn01_nn07"]["baseline"]["expected_cases"] == 7
        assert counts["groups"]["explicit_app_nn08"]["fixed"]["expected_cases"] == 1
    print(json.dumps({"status": "passed", "network": False, "controls": ["unknown is not FP", "partial known source is unsuitable direct", "explicit app control separated", "missing cases retained", "lineage needs observed link and successful parent", "pre-gold metrics stay null"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", type=Path, default=RUN)
    parser.add_argument("--gold", type=Path)
    parser.add_argument("--output", type=Path, default=RUN / "analysis.json")
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        self_check()
    else:
        result = recount(args.run, args.gold)
        save(args.output, result)
        print(json.dumps({"stage": result["stage"], "budget_totals": result["budget_totals"],
                          "observed_article_lineages": result["observed_article_lineages"], "quality_gate": result["quality_gate"]}, ensure_ascii=False))
