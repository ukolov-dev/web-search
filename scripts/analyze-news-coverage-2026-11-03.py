#!/usr/bin/env python3
"""Recount scoped coverage A/B, including conditional recall on a shared observed union."""
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


runner = module("coverage_runner", "scripts/news-coverage-benchmark-2026-11-03.py")
base = module("navigation_analysis", "scripts/analyze-news-navigation-2026-11-03.py")
base.runner = runner
RUN = runner.RUN
load, save = runner.load, runner.save


def model_profile(value, arm):
    verification = value.get("page_verification") or {}
    memory = verification.get("evidence_memory") or {}
    actual = {"model_provider": value.get("model_provider"), "model": value.get("model"),
        "reranker_model": (verification.get("passage_reranker") or {}).get("model"), "evidence_version": memory.get("version")}
    expected = {"model_provider": "openrouter", "model": runner.MODEL,
        "reranker_model": "Qwen3-Reranker-0.6B-4bit",
        "evidence_version": "request-evidence-v11-news" if arm == "baseline" else "request-evidence-v12-coverage"}
    return actual, [f"{key}: {actual[key]!r} != {wanted!r}" for key, wanted in expected.items() if actual[key] != wanted]


def paired_recall(metrics, annotations, case_ids, gold_supplied):
    paired = [case for case in case_ids if all(any(m["case_id"] == case and m["arm"] == arm and m["executed_capture"] for m in metrics) for arm in ("baseline", "fixed"))]
    rows = [r for m in metrics if m["case_id"] in paired for r in m["rows"] if not r["duplicate_url"]]
    tokens = {r["blind_id"] for r in rows}
    positives = {token for token in tokens if annotations.get(token, {}).get("grade") == 2}
    unknown = {token for token in tokens if annotations.get(token, {}).get("grade") is None}
    output = {"scope": "known-positive case/URL union returned by either arm in cases executed by BOTH arms; not full-web or unseen-candidate recall",
        "paired_cases": paired, "paired_case_count": len(paired),
        "known_positive_returned_union": len(positives) if gold_supplied else None,
        "known_positive_blind_ids": sorted(positives) if gold_supplied else [],
        "unknown_returned_union": len(unknown), "arms": {}}
    for arm in ("baseline", "fixed"):
        direct = {r["blind_id"] for r in rows if r["arm"] == arm and r["classification"] == "direct"}
        tp = positives & direct
        output["arms"][arm] = {"tp": len(tp) if gold_supplied else None,
            "fn": len(positives - tp) if gold_supplied else None,
            "direct_recall": len(tp) / len(positives) if gold_supplied and positives else None,
            "tp_blind_ids": sorted(tp) if gold_supplied else [],
            "fn_blind_ids": sorted(positives - tp) if gold_supplied else []}
    left, right = output["arms"]["baseline"]["direct_recall"], output["arms"]["fixed"]["direct_recall"]
    output["direct_recall_delta"] = right - left if left is not None and right is not None else None
    return output


def recount(run, gold_path=None):
    cases = runner.check_protocol()
    case_map = {c["id"]: c for c in cases}
    annotations = base.annotation_map(load(gold_path), case_map) if gold_path else {}
    manifest = load(run / "execution.json") if (run / "execution.json").exists() else {}
    entries = {(x["case"], x["arm"]): x for x in manifest.get("calls", [])}
    metrics = []
    for case in cases:
        for arm in ("baseline", "fixed"):
            path = run / arm / (case["id"] + ".json")
            capture = load(path) if path.exists() else None
            metric = base.case_metrics(case, arm, capture, annotations, gold_path is not None, entries.get((case["id"], arm)))
            value = runner.structured(capture.get("response", {})) if capture else {}
            metric["model_profile"], violations = model_profile(value, arm)
            metric["model_profile_violations"] = violations if capture else []
            errors = metric.get("errors") or []
            if not isinstance(errors, list):
                errors = [errors]
            metric["coverage_unused_requirement_errors"] = sum("evidence unused requirement" in str(error) for error in errors)
            witness_notes = (value.get("page_verification", {}).get("evidence_memory") or {}).get("witness_rejections", [])
            metric["coverage_witness_diagnostic_counts"] = dict(Counter(row.get("reason", "unknown") for row in witness_notes))
            for row in metric["rows"]:
                row["gold_event_cluster"] = annotations.get(row["blind_id"], {}).get("event_cluster")
            metrics.append(metric)
    for case in cases:
        recall = paired_recall(metrics, annotations, [case["id"]], gold_path is not None)
        for metric in (m for m in metrics if m["case_id"] == case["id"]):
            metric["paired_known_positive_returned_union"] = recall["known_positive_returned_union"]
            metric["paired_direct_recall"] = recall["arms"][metric["arm"]]["direct_recall"]
            metric["paired_known_positive_fn"] = recall["arms"][metric["arm"]]["fn"]
    groups = {}
    for group, case_ids in load(runner.PLAN)["groups"].items():
        groups[group] = {}
        for arm in ("baseline", "fixed"):
            selected = [m for m in metrics if m["case_id"] in case_ids and m["arm"] == arm]
            rows = [r for m in selected for r in m["rows"] if not r["duplicate_url"]]
            direct = [r for r in rows if r["classification"] == "direct"]
            fp = [r for r in direct if r["direct_judgement"] == "known_unsuitable_direct"]
            tp = [r for r in direct if r["direct_judgement"] == "known_suitable_direct"]
            unknown = [r for r in direct if r["direct_judgement"] == "unknown_direct"]
            groups[group][arm] = {"expected_cases": len(selected),
                "executed_cases": sum(m["executed_capture"] for m in selected),
                "returned": len(rows), "direct": len(direct), "gold_available": gold_path is not None,
                "qualified_pages": sum(r["gold_grade"] == 2 for r in rows) if gold_path else None,
                "known_suitable_direct": len(tp) if gold_path else None,
                "known_unsuitable_direct": len(fp) if gold_path else None, "unknown_direct": len(unknown),
                "judged_direct_precision": len(tp)/(len(tp)+len(fp)) if tp or fp else None,
                "known_suitable_direct_rows": tp, "known_unsuitable_direct_rows": fp, "unknown_direct_rows": unknown,
                "candidate_news_direct_policy_violations": [r for r in direct if r["candidate_news_direct_policy_violation"]],
                "coverage_unused_requirement_errors": sum(m["coverage_unused_requirement_errors"] for m in selected)}
        left, right = groups[group]["baseline"], groups[group]["fixed"]
        old_fp = {(r["case_id"], r["canonical_url"]) for r in left["known_unsuitable_direct_rows"]}
        groups[group]["new_known_unsuitable_direct_rows"] = [r for r in right["known_unsuitable_direct_rows"] if (r["case_id"], r["canonical_url"]) not in old_fp]
        groups[group]["qualified_pages_delta"] = right["qualified_pages"] - left["qualified_pages"] if gold_path else None
        groups[group]["paired_returned_union_recall"] = paired_recall(metrics, annotations, case_ids, gold_path is not None)
    budget_totals = {arm: {key: sum(m["budget"].get(key, 0) for m in metrics if m["arm"] == arm)
        for key in ("logical_search_calls", "classification_calls", "document_reads", "reranker_calls", "reranker_pairs")} for arm in ("baseline", "fixed")}
    rows = [r for m in metrics for r in m["rows"] if not r["duplicate_url"]]
    supplied = {r["blind_id"] for r in rows}
    missing = sorted(supplied - annotations.keys()) if gold_path else sorted(supplied)
    news = groups["all_news_nc01_nc08"]
    all_captured = sum(m["executed_capture"] for m in metrics) == 16
    quality = {"evaluated": bool(gold_path and not missing and all_captured), "all16_captures": all_captured,
        "gold_annotation_missing_ids": missing, "unused_gold_ids": sorted(annotations.keys() - supplied),
        "new_known_news_false_direct_rows": news["new_known_unsuitable_direct_rows"],
        "candidate_news_direct_policy_violations": news["fixed"]["candidate_news_direct_policy_violations"],
        "candidate_known_app_download_news_direct_rows": [r for r in news["fixed"]["known_unsuitable_direct_rows"] if r["gold_kind"] == "application"],
        "candidate_unknown_news_direct_rows": news["fixed"]["unknown_direct_rows"],
        "model_profile_violations": [{"case_id": m["case_id"], "arm": m["arm"], "violations": m["model_profile_violations"]} for m in metrics if m["model_profile_violations"]],
        "budget_violations": [{"case_id": m["case_id"], "arm": m["arm"], "violations": m["budget_violations"]} for m in metrics if m["budget_violations"]]}
    quality["passed"] = not any(quality[key] for key in ("new_known_news_false_direct_rows", "candidate_news_direct_policy_violations", "candidate_known_app_download_news_direct_rows", "candidate_unknown_news_direct_rows", "model_profile_violations", "budget_violations")) if quality["evaluated"] else None
    recall = news["paired_returned_union_recall"]
    delta = recall["direct_recall_delta"]
    return {"stage": "source_gold_joined" if gold_path else "pre_gold_runtime_recount",
        "gold_sha256": runner.sha(gold_path) if gold_path else None,
        "limits": ["Live sources/HTML not frozen identical inputs", "Queries held out; domains/pages may overlap prior studies", "Conditional returned-union recall is not full-web recall", "Unpaired/unexecuted cases excluded from paired recall and retained explicitly", "Unknown originals excluded from known-positive denominator, unknown direct not FP", "Runtime date/type eligibility is not source gold", "No full model inputs/raw target HTML/usage/cost in public MCP"],
        "groups": groups, "budget_totals": budget_totals,
        "observed_article_lineages": {arm: sum(m["complete_observed_article_lineages"] for m in metrics if m["arm"] == arm) for arm in ("baseline", "fixed")},
        "runtime_eligible_observed_article_lineages": {arm: sum(m["runtime_eligible_observed_article_lineages"] for m in metrics if m["arm"] == arm) for arm in ("baseline", "fixed")},
        "initial_classifiers_valid": {arm: sum(m["classifier_valid"] is True for m in metrics if m["arm"] == arm) for arm in ("baseline", "fixed")},
        "execution_integrity": {"all16_captures": all_captured,
            "all_initial_classifiers_valid": all(m["classifier_valid"] is True for m in metrics),
            "public_model_profiles_match": not quality["model_profile_violations"],
            "both_arms_used_qwen": all(budget_totals[arm]["reranker_calls"] > 0 for arm in ("baseline", "fixed"))},
        "live_usefulness": {"paired_known_positive_union": recall["known_positive_returned_union"],
            "paired_direct_recall_gain_observed": delta > 0 if delta is not None else None,
            "paired_direct_recall_delta": delta, "qualified_pages_delta": news["qualified_pages_delta"],
            "source_qualified_discovered_articles_fixed": sum(m["source_qualified_discovered_articles"] or 0 for m in metrics if m["arm"] == "fixed") if gold_path else None,
            "note": "Guard safety, useful source discovery and direct recall are distinct; zero known positives gives undefined recall."},
        "quality_gate": quality, "cases": metrics}


def self_check():
    case = {"id": "test", "intent": "news", "query": "новости города сегодня"}
    url = "https://example.org/article/"
    result = {"id": "C001", "url": url, "classification": "direct"}
    capture = {"response": {"result": {"structuredContent": {"status": "ok", "results": [result]}}}}
    token = base.blind_id("test", url)
    unknown = base.case_metrics(case, "fixed", capture, {}, True)
    assert unknown["unknown_direct"] == 1 and unknown["known_unsuitable_direct"] == 0
    known = base.case_metrics(case, "fixed", capture, {token: {"case_id": "test", "grade": 2, "kind": "article", "reason": "verified"}}, True)
    missing = base.case_metrics(case, "baseline", None, {}, True)
    assert paired_recall([missing, known], {token: {"grade": 2}}, ["test"], True)["paired_case_count"] == 0
    empty_capture = {"response": {"result": {"structuredContent": {"status": "empty", "results": []}}}}
    absent = base.case_metrics(case, "baseline", empty_capture, {}, True)
    recall = paired_recall([absent, known], {token: {"grade": 2}}, ["test"], True)
    assert recall["known_positive_returned_union"] == 1
    assert recall["arms"]["baseline"]["direct_recall"] == 0 and recall["arms"]["baseline"]["fn"] == 1
    assert recall["arms"]["fixed"]["direct_recall"] == 1 and recall["direct_recall_delta"] == 1
    assert paired_recall([absent, unknown], {}, ["test"], True)["arms"]["fixed"]["direct_recall"] is None
    assert paired_recall([absent, known], {token: {"grade": 2}}, ["test"], False)["known_positive_returned_union"] is None
    profile = {"model_provider": "openrouter", "model": runner.MODEL, "page_verification": {
        "passage_reranker": {"model": "Qwen3-Reranker-0.6B-4bit"}, "evidence_memory": {"version": "request-evidence-v12-coverage"}}}
    assert not model_profile(profile, "fixed")[1] and model_profile(profile, "baseline")[1]
    assert model_profile({"page_verification": {"evidence_memory": None}}, "fixed")[1]
    with tempfile.TemporaryDirectory() as directory:
        counts = recount(Path(directory))
        assert len(counts["cases"]) == 16 and counts["quality_gate"]["passed"] is None
        assert counts["groups"]["current_news_nc01_nc06"]["fixed"]["expected_cases"] == 6
        assert counts["groups"]["all_news_nc01_nc08"]["baseline"]["expected_cases"] == 8
    print(json.dumps({"status": "passed", "network": False, "controls": ["unknown direct is not FP", "unpaired cases not false negatives", "positive only in other arm remains FN", "shared paired union denominator", "zero known positives undefined", "pre-gold denominator null", "all16 cases retained", "current news and controls separated", "public model/version profiles checked", "null memory is not an analysis crash"]}))


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
        print(json.dumps({"stage": value["stage"], "budget_totals": value["budget_totals"],
            "live_usefulness": value["live_usefulness"], "quality_gate": value["quality_gate"]}, ensure_ascii=False))
