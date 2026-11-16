#!/usr/bin/env python3
"""Bounded v11/v12 live comparison. Credentials stay in trusted startup files."""
import argparse
from datetime import datetime, timezone, timedelta
import hashlib
import importlib.util
import json
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("news_navigation_runner", ROOT / "scripts/news-navigation-benchmark-2026-11-03.py")
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
RUN = ROOT / "runs/news-coverage-2026-11-03/benchmark"
CASES = ROOT / "feasibility/news-coverage-cases-v1.json"
PLAN = ROOT / "feasibility/news-coverage-plan-v1.json"
MODEL = base.MODEL
sha, save, load, canonical, structured = base.sha, base.save, base.load, base.canonical, base.structured
base.RUN, base.CASES, base.PLAN = RUN, CASES, PLAN


def check_protocol():
    cases = load(CASES)["cases"]
    assert len(cases) == 8 and len({c["query"] for c in cases}) == 8
    for relative, wanted in load(RUN / "protocol-freeze.json")["sha256"].items():
        assert sha(ROOT / relative) == wanted, f"Protocol changed after freeze: {relative}"
    previous = {c["query"] for c in load(ROOT / "feasibility/news-navigation-cases-v1.json")["cases"]}
    previous.update(["новости Воронежа", "новости Воронежа сегодня 3 ноября 2026",
                     "Воронеж перекрытие дорог транспорт ноябрь 2026",
                     "Воронеж культурные события мероприятия 3 4 ноября 2026"])
    assert not previous & {c["query"] for c in cases}
    for case in cases:
        assert not any(word in case["query"].lower() for word in ("andro.io", "скачать", "download", "приложение"))
        if "сегодня" in case["query"]:
            assert case["published_from"] == case["published_to"] == "2026-11-03"
    assert {c["id"] for c in cases if c["intent"] == "news"} == set(load(PLAN)["groups"]["current_news_nc01_nc06"])
    return cases


def source_validation_plan(captures):
    direct, children, other, failed = [], [], [], set()
    blocked = {"andro.io"}
    for _, _, value in captures:
        for doc in value.get("page_verification", {}).get("documents", []):
            if doc.get("error"):
                failed.add(doc["url"])
            if base.block_reason({"page_verification": {"documents": [doc]}}):
                blocked.update(doc.get("attempted_hosts", []))
                for url in (doc.get("url"), doc.get("final_url")):
                    if url and urlsplit(url).hostname:
                        blocked.add(urlsplit(url).hostname)
        for row in value.get("results", []):
            target = direct if row.get("classification") == "direct" else other
            target.append(row["url"])
        for doc in (value.get("news_navigation") or {}).get("documents", []):
            if doc.get("parent_url") and doc.get("http_status") == 200 and not doc.get("error"):
                children.append(doc["url"])
    wanted, seen = [], set()
    for url in direct + children + other:
        key = canonical(url)
        host = urlsplit(url).hostname or ""
        if key in seen or url in failed or any(host == item or host.endswith("." + item) for item in blocked):
            continue
        seen.add(key)
        wanted.append(url)
        if len(wanted) == 24:
            break
    return {"urls": wanted, "unique_cap": 24, "blocked_hosts": sorted(blocked),
            "already_failed_not_retried": sorted(failed), "requests_made": 0,
            "rule": "direct first, then successful observed children, then other returned URLs; native guarded validation is a separate root-orchestrated phase"}


def make_blind():
    return base.make_blind()


def live():
    cases = check_protocol()
    assert datetime.now(timezone(timedelta(hours=3))).date().isoformat() == "2026-11-03", "Frozen relative-date queries require their original Moscow day"
    binaries = {"baseline": ROOT / "runs/news-coverage-2026-11-03/baseline-web-search",
                "fixed": ROOT / "target/release/web-search"}
    assert all(path.is_file() for path in binaries.values())
    assert not (RUN / "execution.json").exists(), "Never overwrite or silently retry a series"
    for arm in binaries:
        assert not (RUN / arm).exists(), f"Arm folder already exists: {arm}"
    code = {str(path.relative_to(ROOT)): sha(path) for path in (ROOT / "src").glob("*.rs")}
    tooling = {str(path.relative_to(ROOT)): sha(path) for path in [Path(__file__).resolve(), ROOT / "scripts/news-navigation-benchmark-2026-11-03.py"]}
    save(RUN / "code-freeze.json", {"created_utc": datetime.now(timezone.utc).isoformat(),
        "source_sha256": code, "tooling_sha256": tooling,
        "binary_sha256": {arm: sha(path) for arm, path in binaries.items()},
        "protocol_sha256": load(RUN / "protocol-freeze.json")["sha256"]})
    clients, captures = {}, []
    manifest = {"status": "running", "started_utc": datetime.now(timezone.utc).isoformat(),
        "model": MODEL, "reranker": "Qwen3-Reranker-0.6B-4bit", "evidence_memory": True,
        "protocol": "feasibility/news-coverage-plan-v1.json", "cases": cases,
        "input_equality": "live paired; sources and target HTML are not guaranteed identical",
        "known_blocked_host": "andro.io", "calls": []}
    save(RUN / "execution.json", manifest)
    stop = None
    try:
        for arm, binary in binaries.items():
            clients[arm] = base.Client(binary, RUN / arm)
        for ordinal, case in enumerate(cases, 1):
            order = ["baseline", "fixed"] if ordinal % 2 else ["fixed", "baseline"]
            for arm in order:
                if stop:
                    entry = {"case": case["id"], "arm": arm, "status": "not_run_after_block", "reason": stop}
                else:
                    capture = clients[arm].call("ranked_search", {"query": case["query"], "max_results": 10}, case["id"])
                    value = structured(capture["response"])
                    captures.append((arm, case, value))
                    entry = {"case": case["id"], "arm": arm, "status": value.get("status"),
                        "wall_ms": capture["wall_ms"], "rows": len(value.get("results", [])),
                        "budget": value.get("budget"), "errors": value.get("errors", value.get("error"))}
                    stop = base.block_reason(value)
                manifest["calls"].append(entry)
                save(RUN / "execution.json", manifest)
                print(json.dumps(entry, ensure_ascii=False), flush=True)
        save(RUN / "source-validation-input.json", source_validation_plan(captures))
        manifest["blind_unique_rows"] = make_blind()
        manifest["status"] = "completed" if len(captures) == 16 else "partial_block"
        manifest["extra_validation"] = "planned separately; no original reads or model calls performed by this runner"
    except Exception as exc:
        manifest["status"] = "transport_or_environment_failure"
        manifest["failure"] = str(exc)
        raise
    finally:
        manifest["exit_codes"] = {arm: client.close() for arm, client in clients.items()}
        manifest["finished_utc"] = datetime.now(timezone.utc).isoformat()
        manifest["code_unchanged"] = all(sha(ROOT / path) == wanted for path, wanted in code.items())
        manifest["tooling_unchanged"] = all(sha(ROOT / path) == wanted for path, wanted in tooling.items())
        save(RUN / "execution.json", manifest)


def self_check():
    cases = check_protocol()
    assert canonical("https://www.example.org/news/a/?utm_source=x&b=2#hello") == "//example.org/news/a?b=2"
    assert base.block_reason({"errors": ["naver: HTTP 403"]})
    assert base.block_reason({"results": [{"snippet": "CAPTCHA in news"}], "errors": []}) is None
    assert not base.block_reason({"errors": ["C001: fetch_timeout"]})
    sample = [("fixed", cases[0], {"results": [
        {"url": "https://andro.io/app/one", "classification": "direct"},
        {"url": "https://www.andro.io/app/two", "classification": "direct"},
        {"url": "https://example.org/article/", "classification": "direct"}],
        "page_verification": {"documents": [{"url": "https://example.org/failed/", "error": "fetch_timeout"},
            {"url": "https://origin.example/redirect", "error": "source_blocked", "attempted_hosts": ["origin.example", "blocked.example"]}]},
        "news_navigation": {"documents": [
            {"url": "https://example.org/failed/", "parent_url": "https://example.org/", "http_status": 200},
            {"url": "https://example.org/child/", "parent_url": "https://example.org/", "http_status": 200}]}})]
    plan = source_validation_plan(sample)
    assert plan["urls"] == ["https://example.org/article/", "https://example.org/child/"]
    assert plan["requests_made"] == 0 and {"andro.io", "origin.example", "blocked.example"} <= set(plan["blocked_hosts"])
    print(json.dumps({"status": "passed", "network": False, "cases": len(cases),
        "controls": ["new exact queries", "today date bound", "known block excluded", "failed originals not retried", "source plan only, no requests"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--blind", action="store_true")
    parser.add_argument("--live", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        self_check()
    elif args.blind:
        print(json.dumps({"unique_source_rows": make_blind()}))
    elif args.live:
        live()
    else:
        parser.error("Use --self-check, --blind or explicit --live")
