#!/usr/bin/env python3
"""Source-gold counter: primary successful READ+returned union, returned-only secondary."""
import argparse
from collections import Counter
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def module(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


runner = module("dates_runner", "scripts/news-dates-benchmark-2026-11-03.py")
base = module("navigation_analysis", "scripts/analyze-news-navigation-2026-11-03.py")
base.runner = runner
RUN = runner.RUN
load, save = runner.load, runner.save


def annotations(gold, cases):
    rows = base.annotation_map(gold, cases)
    for identifier, row in rows.items():
        assert row.get("grade") is None or type(row["grade"]) is int, f"Noninteger grade: {identifier}"
        assert isinstance(row.get("source_reviewed"), bool) and isinstance(row.get("date_verified"), bool), f"Missing source flags: {identifier}"
        assert isinstance(row.get("url"), str) and runner.token(row["case_id"], row["url"]) == identifier, f"Gold URL/ID mismatch: {identifier}"
        assert isinstance(row.get("evidence"), list), f"Gold evidence list missing: {identifier}"
    return rows


def model_profile(value, arm):
    page = value.get("page_verification") or {}
    actual = {"model_provider": value.get("model_provider"), "model": value.get("model"),
        "reranker_model": (page.get("passage_reranker") or {}).get("model"),
        "evidence_version": (page.get("evidence_memory") or {}).get("version")}
    expected = {"model_provider": "openrouter", "model": runner.MODEL,
        "reranker_model": "Qwen3-Reranker-0.6B-4bit", "evidence_version": load(runner.PLAN)["evidence_versions"][arm]}
    return actual, [f"{key}: {actual[key]!r} != {wanted!r}" for key, wanted in expected.items() if actual[key] != wanted]


def observed_rows(case, arm, value, gold):
    rows = {}
    for observation in runner.observations(case, arm, value):
        identifier, hidden = observation["blind_id"], observation["hidden"]
        annotation = gold.get(identifier, {})
        row = rows.setdefault(identifier, {"blind_id": identifier, "case_id": case["id"], "arm": arm,
            "url": observation["card"]["url"], "read": False, "returned": False, "trace_ids": [],
            "gold_grade": annotation.get("grade"), "gold_kind": annotation.get("kind"),
            "gold_event_cluster": annotation.get("event_cluster"), "gold_publisher_cluster": annotation.get("publisher_cluster")})
        row["read"] |= hidden["read"]
        row["returned"] |= hidden["returned"]
        row["trace_ids"].append(hidden["trace_id"])
    return list(rows.values())


def paired_recall(metrics, gold, case_ids, supplied, returned_only=False):
    paired = [case for case in case_ids if all(any(m["case_id"] == case and m["arm"] == arm and m["executed_capture"] for m in metrics) for arm in ("baseline", "fixed"))]
    selected = [m for m in metrics if m["case_id"] in paired]
    population = {r["blind_id"] for m in selected for r in m["observed_rows"] if not returned_only or r["returned"]}
    positives = {identifier for identifier in population if gold.get(identifier, {}).get("grade") == 2}
    output = {"scope": "paired returned-only union (secondary)" if returned_only else "paired successful READ+returned known-positive union (primary)",
        "paired_cases": paired, "paired_case_count": len(paired), "observed_union": len(population),
        "known_positive_union": len(positives) if supplied else None, "known_positive_blind_ids": sorted(positives),
        "unknown_or_unannotated_union": sum(gold.get(identifier, {}).get("grade") is None for identifier in population), "arms": {}}
    for arm in ("baseline", "fixed"):
        direct = {r["blind_id"] for m in selected if m["arm"] == arm for r in m["rows"] if not r["duplicate_url"] and r["classification"] == "direct"}
        tp = positives & direct
        output["arms"][arm] = {"tp": len(tp) if supplied else None, "fn": len(positives - tp) if supplied else None,
            "direct_recall": len(tp) / len(positives) if supplied and positives else None,
            "tp_blind_ids": sorted(tp), "fn_blind_ids": sorted(positives - tp)}
    left, right = [output["arms"][arm]["direct_recall"] for arm in ("baseline", "fixed")]
    output["direct_recall_delta"] = right - left if left is not None and right is not None else None
    positive_gold = [gold[i] for i in positives]
    output["clusters"] = {key: {"distinct_named": len({r[key] for r in positive_gold if r.get(key)}),
        "names": sorted({r[key] for r in positive_gold if r.get(key)}), "unclustered_case_urls": sum(not r.get(key) for r in positive_gold)}
        for key in ("event_cluster", "publisher_cluster")}
    return output


def recount(run, gold_path=None):
    cases = runner.check_protocol()
    case_map = {case["id"]: case for case in cases}
    gold = annotations(load(gold_path), case_map) if gold_path else {}
    manifest = load(run / "execution.json") if (run / "execution.json").exists() else {}
    entries = {(r["case"], r["arm"]): r for r in manifest.get("calls", [])}
    metrics = []
    for case in cases:
        for arm in ("baseline", "fixed"):
            path = run / arm / (case["id"] + ".json")
            capture = load(path) if path.exists() else None
            value = runner.structured(capture.get("response", {})) if capture else {}
            metric = base.case_metrics(case, arm, capture, gold, gold_path is not None, entries.get((case["id"], arm)))
            metric["executed_capture"] = bool(capture and not capture.get("transport_error") and isinstance(value.get("results"), list)
                and isinstance(value.get("budget"), dict) and value.get("model") and value.get("query") == case["query"])
            metric["input_query_matches"] = value.get("query") == case["query"]
            metric["observed_rows"] = observed_rows(case, arm, value, gold)
            metric["gold_complete_for_observed"] = bool(gold_path and all(r["blind_id"] in gold for r in metric["observed_rows"]))
            metric["observed_unknown_sources"] = sum(r["gold_grade"] is None for r in metric["observed_rows"])
            metric["read_not_returned_known_positives"] = [r for r in metric["observed_rows"] if r["read"] and not r["returned"] and r["gold_grade"] == 2]
            metric["model_profile"], violations = model_profile(value, arm)
            metric["model_profile_violations"] = violations if metric["executed_capture"] else []
            metric["source_block"] = runner.block_reason(value)
            memory = (value.get("page_verification") or {}).get("evidence_memory") or {}
            metric["coverage_witness_diagnostic_counts"] = dict(Counter(r.get("reason", "unknown") for r in memory.get("witness_rejections", [])))
            metrics.append(metric)
    for case in cases:
        recall = paired_recall(metrics, gold, [case["id"]], gold_path is not None)
        for metric in (m for m in metrics if m["case_id"] == case["id"]):
            metric["primary_known_positive_union"] = recall["known_positive_union"]
            metric["primary_direct_recall"] = recall["arms"][metric["arm"]]["direct_recall"]
            metric["primary_known_positive_fn"] = recall["arms"][metric["arm"]]["fn"]
    groups = {}
    for name, case_ids in load(runner.PLAN)["groups"].items():
        group = {}
        for arm in ("baseline", "fixed"):
            selected = [m for m in metrics if m["case_id"] in case_ids and m["arm"] == arm]
            rows = [r for m in selected for r in m["rows"] if not r["duplicate_url"]]
            observed = [r for m in selected for r in m["observed_rows"]]
            directs = [r for r in rows if r["classification"] == "direct"]
            tp = [r for r in directs if r["gold_grade"] == 2]
            fp = [r for r in directs if r["gold_grade"] in (0, 1)]
            unknown = [r for r in directs if r["gold_grade"] is None]
            group[arm] = {"expected_cases": len(selected), "executed_cases": sum(m["executed_capture"] for m in selected),
                "returned": len(rows), "direct": len(directs), "gold_available": gold_path is not None,
                "qualified_returned_pages": sum(r["gold_grade"] == 2 for r in rows) if gold_path else None,
                "observed_read_returned_sources": len(observed), "successful_read_sources": sum(r["read"] for r in observed),
                "known_qualified_observed_sources": sum(r["gold_grade"] == 2 for r in observed) if gold_path else None,
                "unknown_or_unannotated_observed_sources": sum(r["gold_grade"] is None for r in observed),
                "known_suitable_direct": len(tp) if gold_path else None, "known_unsuitable_direct": len(fp) if gold_path else None,
                "unknown_direct": len(unknown), "judged_direct_precision": len(tp) / (len(tp) + len(fp)) if tp or fp else None,
                "known_suitable_direct_rows": tp, "known_unsuitable_direct_rows": fp, "unknown_direct_rows": unknown,
                "candidate_news_direct_policy_violations": [r for r in directs if r["candidate_news_direct_policy_violation"]],
                "read_not_returned_known_positive_rows": [r for m in selected for r in m["read_not_returned_known_positives"]]}
        old_fp = {r["blind_id"] for r in group["baseline"]["known_unsuitable_direct_rows"]}
        group["new_known_unsuitable_direct_rows"] = [r for r in group["fixed"]["known_unsuitable_direct_rows"] if r["blind_id"] not in old_fp]
        group["qualified_returned_pages_delta"] = group["fixed"]["qualified_returned_pages"] - group["baseline"]["qualified_returned_pages"] if gold_path else None
        group["primary_paired_read_returned_recall"] = paired_recall(metrics, gold, case_ids, gold_path is not None)
        group["secondary_paired_returned_only_recall"] = paired_recall(metrics, gold, case_ids, gold_path is not None, returned_only=True)
        groups[name] = group
    supplied = {r["blind_id"] for m in metrics for r in m["observed_rows"]}
    missing = sorted(supplied - gold.keys())
    unused = sorted(gold.keys() - supplied)
    all_captured = sum(m["executed_capture"] for m in metrics) == 24
    expected_order = [(case["id"], arm) for ordinal, case in enumerate(cases, 1)
        for arm in (["baseline", "fixed"] if ordinal % 2 else ["fixed", "baseline"])]
    actual_order = [(row["case"], row["arm"]) for row in manifest.get("calls", [])]
    integrity = [key for key in ("code_unchanged", "tooling_unchanged") if manifest.get(key) is not True]
    if manifest.get("status") != "completed":
        integrity.append("cohort_not_completed")
    if actual_order != expected_order:
        integrity.append("cohort_order_or_entries_mismatch")
    news = groups["all_news_nd01_nd12"]
    quality = {"evaluated": bool(gold_path and not missing and not unused and all_captured), "all24_valid_captures": all_captured,
        "gold_annotation_missing_ids": missing, "unused_gold_ids": unused,
        "new_known_news_false_direct_rows": news["new_known_unsuitable_direct_rows"],
        "candidate_news_direct_policy_violations": news["fixed"]["candidate_news_direct_policy_violations"],
        "candidate_unknown_news_direct_rows": news["fixed"]["unknown_direct_rows"],
        "model_profile_violations": [{"case_id": m["case_id"], "arm": m["arm"], "violations": m["model_profile_violations"]} for m in metrics if m["model_profile_violations"]],
        "budget_violations": [{"case_id": m["case_id"], "arm": m["arm"], "violations": m["budget_violations"]} for m in metrics if m["budget_violations"]],
        "invalid_initial_classifiers": [{"case_id": m["case_id"], "arm": m["arm"]} for m in metrics if m["executed_capture"] and m["classifier_valid"] is not True],
        "source_blocks": [{"case_id": m["case_id"], "arm": m["arm"], "reason": m["source_block"]} for m in metrics if m["source_block"]],
        "execution_integrity_violations": integrity}
    gates = ("new_known_news_false_direct_rows", "candidate_news_direct_policy_violations", "candidate_unknown_news_direct_rows",
        "model_profile_violations", "budget_violations", "invalid_initial_classifiers", "source_blocks", "execution_integrity_violations")
    quality["passed"] = not any(quality[key] for key in gates) if quality["evaluated"] else None
    primary = news["primary_paired_read_returned_recall"]
    delta = primary["direct_recall_delta"]
    return {"stage": "source_gold_joined" if gold_path else "pre_gold_runtime_recount", "gold_sha256": runner.sha(gold_path) if gold_path else None,
        "transport": "native CLI shared policy::Session::run/public_result, not persistent MCP",
        "limits": ["Known-positive conditional successful READ+returned union is not candidate-pool/full-web recall", "Unknown source originals/dates excluded from known-positive denominator and counted explicitly", "Live arm variants may differ despite exact primary artifacts", "Exact queries held out; domains/URLs can overlap development; nd08 targeted source-aware sensitivity", "Pooled case/URL score and distinct event/publisher clusters are separate"],
        "groups": groups, "cases": metrics, "observed_unique_case_url_union": len(supplied),
        "budget_totals": {arm: {key: sum(m["budget"].get(key, 0) for m in metrics if m["arm"] == arm) for key in
            ("logical_search_calls", "classification_calls", "document_reads", "reranker_calls", "reranker_pairs")} for arm in ("baseline", "fixed")},
        "quality_gate": quality, "live_usefulness": {"primary_known_positive_union": primary["known_positive_union"],
            "primary_direct_recall_delta": delta, "primary_direct_recall_gain_observed": delta > 0 if delta is not None else None,
            "note": "Safety and recall usefulness separate; returned-only secondary score cannot replace the primary READ+returned denominator."}}


def self_check():
    case = {"id": "test", "intent": "news", "query": "новости города сегодня", "published_from": "2026-11-03", "published_to": "2026-11-03"}
    url = "https://example.org/a/"
    identifier = runner.token("test", url)
    value = {"results": [], "news_navigation": {"documents": [{"id": "C009", "url": url, "http_status": 200, "text_hash": "hash"}]}}
    observed = observed_rows(case, "baseline", value, {})
    assert len(observed) == 1 and observed[0]["read"] and not observed[0]["returned"]
    left = {"case_id": "test", "arm": "baseline", "executed_capture": True, "rows": [], "observed_rows": observed}
    right = {"case_id": "test", "arm": "fixed", "executed_capture": True,
        "rows": [{"blind_id": identifier, "duplicate_url": False, "classification": "direct"}],
        "observed_rows": [{"blind_id": identifier, "returned": True, "read": True}]}
    gold = {identifier: {"grade": 2, "event_cluster": "story", "publisher_cluster": "publisher"}}
    recall = paired_recall([left, right], gold, ["test"], True)
    assert recall["known_positive_union"] == 1 and recall["arms"]["baseline"]["fn"] == 1
    assert recall["arms"]["baseline"]["direct_recall"] == 0 and recall["arms"]["fixed"]["direct_recall"] == 1
    both_read = {**right, "rows": [], "observed_rows": [{"blind_id": identifier, "returned": False, "read": True}]}
    assert paired_recall([left, both_read], gold, ["test"], True)["arms"]["fixed"]["fn"] == 1
    assert paired_recall([left, both_read], gold, ["test"], True, True)["arms"]["fixed"]["direct_recall"] is None
    assert paired_recall([dict(left, executed_capture=False), right], gold, ["test"], True)["paired_case_count"] == 0
    assert paired_recall([left, right], {}, ["test"], True)["arms"]["fixed"]["direct_recall"] is None
    assert paired_recall([left, right], gold, ["test"], False)["known_positive_union"] is None
    unknown = base.case_metrics(case, "fixed", {"response": {"result": {"structuredContent": {"results": [{"id": "C009", "url": url, "classification": "direct"}]}}}}, {}, True)
    assert unknown["unknown_direct"] == 1 and unknown["known_unsuitable_direct"] == 0
    full = {"blind_id": identifier, "case_id": "test", "url": url, "grade": 2, "kind": "article", "published_date": "2026-11-03", "source_reviewed": True, "date_verified": True, "reason": "Own source publication", "evidence": []}
    assert annotations({"rows": [full]}, {"test": case})
    for bad in [dict(full, blind_id="wrong"), dict(full, published_date="2026-11-02"), dict(full, published_date="2030-11-03"), dict(full, source_reviewed=False)]:
        try:
            annotations({"rows": [bad]}, {"test": case})
        except AssertionError:
            pass
        else:
            raise AssertionError("Invalid source gold accepted")
    with tempfile.TemporaryDirectory() as directory:
        run = Path(directory)
        result = recount(run)
        assert len(result["cases"]) == 24 and result["quality_gate"]["passed"] is None
        assert result["groups"]["all_news_nd01_nd12"]["baseline"]["expected_cases"] == 12
        actual_cases = runner.check_protocol()
        calls = []
        for ordinal, actual in enumerate(actual_cases, 1):
            for arm in (["baseline", "fixed"] if ordinal % 2 else ["fixed", "baseline"]):
                page = {"passage_reranker": {"model": "Qwen3-Reranker-0.6B-4bit", "calls": []},
                    "evidence_memory": {"version": load(runner.PLAN)["evidence_versions"][arm]}}
                fixture = {"query": actual["query"], "status": "ok", "results": [], "classifier_valid": True,
                    "model_provider": "openrouter", "model": runner.MODEL, "page_verification": page,
                    "budget": {"logical_search_calls": 2, "classification_calls": 1, "document_reads": 0, "reranker_calls": 0, "source_context_max": 100}, "elapsed_ms": 1}
                assert not model_profile(fixture, arm)[1]
                if actual == actual_cases[0]:
                    fixture["news_navigation"] = {"network_reads": 1, "documents": value["news_navigation"]["documents"]}
                    fixture["budget"]["document_reads"] = 1
                save(run / arm / (actual["id"] + ".json"), {"response": {"result": {"structuredContent": fixture}}})
                calls.append({"case": actual["id"], "arm": arm})
        save(run / "execution.json", {"status": "completed", "code_unchanged": True, "tooling_unchanged": True, "calls": calls})
        fixture_gold = dict(full, case_id=actual_cases[0]["id"], blind_id=runner.token(actual_cases[0]["id"], url))
        save(run / "gold.json", {"rows": [fixture_gold]})
        checked = recount(run, run / "gold.json")
        assert checked["quality_gate"]["all24_valid_captures"] and checked["quality_gate"]["passed"] is True
        assert checked["groups"]["all_news_nd01_nd12"]["primary_paired_read_returned_recall"]["arms"]["baseline"]["fn"] == 1
    assert model_profile({"page_verification": {"evidence_memory": None}}, "fixed")[1]
    print(json.dumps({"status": "passed", "network": False, "controls": ["nonreturned positive read is primaryFN", "both-arm read-only positive denominator retained", "returned-only secondary cannot hide primaryFN", "unpaired cases excluded", "unknown direct notFP", "zero denominator undefined", "pre-gold denominator null", "source gold URL/ID/datebound guards", "all24 cases retained", "null memory handled"]}))


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
        value = recount(args.run, args.gold)
        save(args.output, value)
        print(json.dumps({"stage": value["stage"], "budget_totals": value["budget_totals"], "live_usefulness": value["live_usefulness"], "quality_gate": value["quality_gate"]}, ensure_ascii=False))
