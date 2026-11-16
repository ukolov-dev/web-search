#!/usr/bin/env python3
"""NEW12 native CLI v12/v13 pair; reuse public MCP schema and existing guards."""
import argparse
from datetime import datetime, timezone, timedelta
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("news_navigation_runner", ROOT / "scripts/news-navigation-benchmark-2026-11-03.py")
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
RUN = ROOT / "runs/news-dates-2026-11-03/benchmark"
CASES = ROOT / "feasibility/news-dates-cases-v1.json"
PLAN = ROOT / "feasibility/news-dates-plan-v1.json"
MODEL = base.MODEL
sha, save, load, canonical, structured = base.sha, base.save, base.load, base.canonical, base.structured


def token(case_id, url):
    return hashlib.sha256((case_id + "\n" + canonical(url)).encode()).hexdigest()[:16]


def check_protocol():
    cases = load(CASES)["cases"]
    assert len(cases) == 12 and len({c["query"] for c in cases}) == 12
    for relative, wanted in load(RUN / "protocol-freeze.json")["sha256"].items():
        assert sha(ROOT / relative) == wanted, f"Protocol changed after freeze: {relative}"
    previous = set(load(RUN / "prior-query-audit.json")["prior_queries"])
    for case in cases:
        assert " ".join(case["query"].casefold().split()) not in previous
        assert not any(w in case["query"].lower() for w in ("andro.io", "скачать", "download", "приложение"))
        if "сегодня" in case["query"]:
            assert case["published_from"] == case["published_to"] == "2026-11-03"
    assert [c["id"] for c in cases] == [f"nd{i:02d}" for i in range(1, 13)]
    return cases


def documents(value):
    for surface in ("news_navigation", "page_verification"):
        for ordinal, doc in enumerate((value.get(surface) or {}).get("documents", []), 1):
            yield surface, ordinal, doc


def successful(doc):
    return 200 <= (doc.get("http_status") or 0) < 300 and bool(doc.get("text_hash")) and not doc.get("error")


def blocked_host(host):
    return host.lower() == "andro.io" or host.lower().endswith(".andro.io")


def block_reason(value):
    reason = base.block_reason(value)
    if reason:
        return reason
    for _, _, doc in documents(value):
        reason = base.block_reason({"page_verification": {"documents": [doc]}})
        if reason:
            return reason
        if doc.get("http_status") in (202, 403, 429):
            return f"source HTTP {doc['http_status']}: {doc.get('url')}"
        hosts = list(doc.get("attempted_hosts") or [])
        hosts.extend(urlsplit(u).hostname or "" for u in [doc.get("url", ""), doc.get("final_url", "")] + (doc.get("hops") or []))
        if any(blocked_host(host) for host in hosts):
            return "known_blocked_host_contact: andro.io"
    return None


def native_doc(native, candidate_id):
    if not native or not candidate_id:
        return None
    for suffix in (f"*-{candidate_id}-news-document.json", f"*-{candidate_id}-document.json"):
        paths = sorted(native.glob(suffix))
        if paths:
            return load(paths[0])
    return None


def observations(case, arm, value, native=None):
    """Source text and hidden trace provenance; no runtime type filter for reads."""
    output = []
    for rank, result in enumerate(value.get("results", []), 1):
        output.append({"blind_id": token(case["id"], result["url"]),
            "card": {key: result.get(key) for key in ("url", "title", "snippet")},
            "hidden": {"arm": arm, "case_id": case["id"], "trace_id": f"{arm}:{case['id']}:returned:{rank}:{result.get('id')}",
                "candidate_id": result.get("id"), "classification": result.get("classification"), "rank": rank,
                "returned": True, "read": False}})
    for surface, ordinal, doc in documents(value):
        if not successful(doc) or not doc.get("url"):
            continue
        source = native_doc(native, doc.get("id")) or {}
        meta = (doc.get("news") or {}).get("metadata") or {}
        card = {"url": doc["url"], "title": source.get("title") or source.get("page_title") or meta.get("title"),
                "snippet": (source.get("text") or "")[:1500] or None}
        output.append({"blind_id": token(case["id"], doc["url"]), "card": card,
            "hidden": {"arm": arm, "case_id": case["id"], "trace_id": f"{arm}:{case['id']}:{surface}:{ordinal}:{doc.get('id')}",
                "candidate_id": doc.get("id"), "returned": False, "read": True,
                "text_hash": doc.get("text_hash"), "html_hash": doc.get("html_hash"), "final_url": doc.get("final_url"),
                "source_surface": surface}})
    return output


def make_blind(run=RUN):
    rows, mapping = {}, {}
    for case in load(CASES)["cases"]:
        for arm in ("baseline", "fixed"):
            path = run / arm / (case["id"] + ".json")
            if not path.exists():
                continue
            capture = load(path)
            value = structured(capture.get("response", {}))
            native = ROOT / capture["native_directory"] if capture.get("native_directory") else None
            for observed in observations(case, arm, value, native):
                identifier = observed["blind_id"]
                row = rows.setdefault(identifier, {"blind_id": identifier, **{key: case[key] for key in
                    ("id", "query", "intent", "place", "topic", "published_from", "published_to") if key != "id"},
                    "case_id": case["id"], "source_cards": []})
                if case.get("site"):
                    row["site"] = case["site"]
                if observed["card"] not in row["source_cards"]:
                    row["source_cards"].append(observed["card"])
                mapping.setdefault(identifier, []).append(observed["hidden"])
    save(run / "blind/source-cards.json", {"scope": "Successful READ plus returned case/URL union, no arm/prediction/read-or-returned flags",
        "limits": "Cards alone do not establish source/date qualification; exact primary native originals supplied separately; all unknowns retained.",
        "rows": [rows[k] for k in sorted(rows)]})
    save(run / "hidden/arm-map.json", mapping)
    return len(rows)


def source_validation_plan(captures):
    """Optional unread URLs only: never re-fetch captured READ originals."""
    read, failed, blocked, direct, other = set(), set(), {"andro.io"}, [], []
    stop = None
    for _, _, value in captures:
        stop = stop or block_reason(value)
        for _, _, doc in documents(value):
            if doc.get("url") and successful(doc):
                read.add(canonical(doc["url"]))
                if doc.get("final_url"):
                    read.add(canonical(doc["final_url"]))
            if doc.get("error") or (doc.get("http_status") and not 200 <= doc["http_status"] < 300):
                failed.update(canonical(url) for url in [doc.get("url"), doc.get("final_url")] if url)
            if base.block_reason({"page_verification": {"documents": [doc]}}) or doc.get("http_status") in (202, 403, 429):
                blocked.update(doc.get("attempted_hosts") or [])
                blocked.update(urlsplit(url).hostname for url in [doc.get("url"), doc.get("final_url")] + (doc.get("hops") or []) if url and urlsplit(url).hostname)
        for result in value.get("results", []):
            (direct if result.get("classification") == "direct" else other).append(result["url"])
    wanted, seen = [], set()
    for url in direct + other:
        key, host = canonical(url), urlsplit(url).hostname or ""
        if key in seen or key in read or key in failed or any(host == h or host.endswith("." + h) for h in blocked):
            continue
        seen.add(key)
        wanted.append(url)
    return {"urls": wanted[:8] if not stop else [], "unique_cap": 8, "unread_over_cap": wanted[8:], "suppressed_after_source_block": stop,
        "captured_read_canonical_urls": sorted(read), "already_failed_not_retried": sorted(failed),
        "blocked_hosts": sorted(blocked), "requests_made": 0,
        "rule": "Optional unread returned originals only; direct first; READ HTML/docs already captured during primary at zero extraHTTP"}


def assert_frozen(source, tooling, binaries):
    check_protocol()
    actual_source = {str(path.relative_to(ROOT)): sha(path) for path in (ROOT / "src").glob("*.rs")}
    assert actual_source == source, "Source contents/file set changed after freeze"
    for relative, wanted in tooling.items():
        assert sha(ROOT / relative) == wanted, f"Tooling changed: {relative}"
    for path, wanted in binaries.items():
        assert sha(Path(path)) == wanted, f"Binary changed: {path}"


def native_call(binary, arm, case):
    folder = RUN / arm
    folder.mkdir(exist_ok=True)
    native = folder / (case["id"] + "-native")
    assert not native.exists(), "Never overwrite a native run"
    command = [str(binary), "ranked-search", case["query"], "--evidence-memory", "--model", MODEL,
        "--openrouter-key-file", os.path.expanduser("~/.config/web-search/openrouter.key"),
        "--openrouter-proxy", "http://127.0.0.1:10808", "--reranker-settings", os.path.expanduser("~/.omlx/settings.json"),
        "--output", str(native), "--live"]
    started = time.monotonic()
    capture = {"request": {"transport": "native CLI; shared policy::Session::run/public_result", "query": case["query"], "max_results": 10},
        "native_directory": str(native.relative_to(ROOT))}
    with (folder / (case["id"] + ".stdout.log")).open("w") as stdout, (folder / (case["id"] + ".stderr.log")).open("w") as stderr:
        try:
            process = subprocess.run(command, cwd=ROOT, stdout=stdout, stderr=stderr, timeout=210)
            capture["exit_code"] = process.returncode
        except subprocess.TimeoutExpired:
            capture["exit_code"], capture["transport_error"] = None, "native CLI exceeded210s; process killed, no retry"
    if not capture.get("transport_error") and capture["exit_code"] == 0:
        try:
            value = json.loads((folder / (case["id"] + ".stdout.log")).read_text())
            capture["response"] = {"result": {"structuredContent": value}}
        except (ValueError, OSError) as error:
            capture["transport_error"] = f"Native output decode failure: {error}"
    elif not capture.get("transport_error"):
        capture["transport_error"] = f"Native CLI exit{capture['exit_code']}; stderr retained separately"
    capture.update(observed_utc=datetime.now(timezone.utc).isoformat(), wall_ms=round((time.monotonic() - started) * 1000))
    if native.exists():
        save(native / "artifact-freeze.json", {"sha256": {str(path.relative_to(native)): sha(path) for path in sorted(native.rglob("*")) if path.is_file()},
            "scope": "Exact primary native source/model outputs; no later source fetch or extraction"})
    save(folder / (case["id"] + ".json"), capture)
    return capture


def live():
    cases = check_protocol()
    assert datetime.now(timezone(timedelta(hours=3))).date().isoformat() == "2026-11-03", "Frozen today cases require original Moscow day"
    assert not (RUN / "execution.json").exists() and not any((RUN / arm).exists() for arm in ("baseline", "fixed")), "Never overwrite/retry cohort"
    tooling_freeze = load(RUN / "tooling-freeze.json")
    tooling = tooling_freeze["sha256"]
    source = {str(p.relative_to(ROOT)): sha(p) for p in (ROOT / "src").glob("*.rs")}
    binaries = {"baseline": ROOT / "runs/news-dates-2026-11-03/baseline-web-search", "fixed": ROOT / "target/release/web-search"}
    binary_hashes = {str(p): sha(p) for p in binaries.values()}
    profile = {"model_provider": "openrouter", "model": MODEL, "reranker": "Qwen3-Reranker-0.6B-4bit", "evidence_versions": load(PLAN)["evidence_versions"]}
    assert_frozen(source, tooling, binary_hashes)
    save(RUN / "code-freeze.json", {"created_utc": datetime.now(timezone.utc).isoformat(), "transport": "native CLI fresh Session per case/arm",
        "source_sha256": source, "tooling_sha256": tooling, "binary_sha256": {arm: binary_hashes[str(path)] for arm, path in binaries.items()},
        "protocol_sha256": load(RUN / "protocol-freeze.json")["sha256"], "model_profile": profile})
    manifest = {"status": "running", "started_utc": datetime.now(timezone.utc).isoformat(), **profile, "cases": cases,
        "protocol": "feasibility/news-dates-plan-v1.json", "transport": "native CLI shared policy::Session::run/public_result; fresh Session/process per case/arm",
        "input_equality": "Live sources may differ; exact primary raw source and model artifacts retained", "calls": []}
    save(RUN / "execution.json", manifest)
    captures, stop = [], None
    try:
        for ordinal, case in enumerate(cases, 1):
            for arm in (["baseline", "fixed"] if ordinal % 2 else ["fixed", "baseline"]):
                if stop:
                    entry = {"case": case["id"], "arm": arm, "status": "not_run_after_stop", "reason": stop}
                else:
                    assert_frozen(source, tooling, binary_hashes)
                    capture = native_call(binaries[arm], arm, case)
                    value = structured(capture.get("response", {}))
                    captures.append((arm, case, value))
                    entry = {"case": case["id"], "arm": arm, "status": value.get("status"), "exit_code": capture["exit_code"],
                        "wall_ms": capture["wall_ms"], "rows": len(value.get("results", [])), "budget": value.get("budget"),
                        "errors": value.get("errors", value.get("error")), "transport_error": capture.get("transport_error")}
                    stop = capture.get("transport_error") or block_reason(value)
                    assert_frozen(source, tooling, binary_hashes)
                manifest["calls"].append(entry)
                save(RUN / "execution.json", manifest)
                print(json.dumps(entry, ensure_ascii=False), flush=True)
        manifest["blind_unique_rows"] = make_blind()
        save(RUN / "optional-unread-validation-input.json", source_validation_plan(captures))
        manifest["status"] = "completed" if not stop else "partial_stopped"
        manifest["stop_reason"] = stop
    except Exception as error:
        manifest["status"], manifest["failure"] = "transport_or_environment_failure", str(error)
        raise
    finally:
        manifest["finished_utc"] = datetime.now(timezone.utc).isoformat()
        manifest["code_unchanged"] = {str(p.relative_to(ROOT)): sha(p) for p in (ROOT / "src").glob("*.rs")} == source
        manifest["tooling_unchanged"] = all(sha(ROOT / p) == h for p, h in tooling.items())
        save(RUN / "execution.json", manifest)


def self_check():
    cases = check_protocol()
    sample = {"results": [{"id": "C002", "url": "https://example.org/a/", "title": "A", "snippet": "Body", "classification": "direct"}],
        "news_navigation": {"documents": [{"id": "C003", "url": "https://example.org/b/", "http_status": 200, "text_hash": "hash", "news": {"metadata": {"kind": "unknown"}}},
            {"id": "C004", "url": "https://example.org/failed", "error": "fetch_timeout"}]}}
    obs = observations(cases[0], "fixed", sample)
    assert len(obs) == 2 and obs[1]["hidden"]["read"] and not obs[1]["hidden"]["returned"]
    assert obs[0]["hidden"]["trace_id"] != obs[1]["hidden"]["trace_id"]
    assert not {"arm", "classification", "read", "returned", "kind", "date_check"} & obs[1]["card"].keys()
    sample["results"].extend([{"url": "https://example.org/b/"}, {"url": "https://www.andro.io/a", "classification": "direct"}, {"url": "https://example.org/failed"}])
    plan = source_validation_plan([("fixed", cases[0], sample)])
    assert plan["urls"] == ["https://example.org/a/"] and plan["requests_made"] == 0
    assert block_reason({"results": [{"snippet": "CAPTCHA news"}]}) is None
    blocked = {"results": [{"url": "https://safe.example/a"}], "news_navigation": {"documents": [{"error": "source_blocked", "url": "https://example.org/a"}]}}
    assert block_reason(blocked) and source_validation_plan([("baseline", cases[0], blocked)])["urls"] == []
    with tempfile.TemporaryDirectory() as directory:
        run = Path(directory)
        save(run / "fixed" / (cases[0]["id"] + ".json"), {"response": {"result": {"structuredContent": sample}}})
        count = make_blind(run)
        blind = load(run / "blind/source-cards.json")
        assert count == 4 and any(row["blind_id"] == token(cases[0]["id"], "https://example.org/b/") for row in blind["rows"])
        assert not any(key in json.dumps(blind) for key in ('"classification"', '"returned"', '"arm"', '"date_check"'))
    print(json.dumps({"status": "passed", "network": False, "cases": len(cases), "controls": ["all12 exact new queries", "date bounds frozen", "successful nonreturned read retained regardless runtime kind", "trace IDs distinct", "blind labels hidden", "unread-only plan excludes READ/failed/blocked", "block stops inspect errors not source mentions"]}))


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
