#!/usr/bin/env python3
"""Bounded paired live MCP check; never reads or archives credential contents."""
import argparse
from datetime import datetime, timezone, timedelta
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import threading
import time
from urllib.parse import urlsplit, urlunsplit, parse_qsl, urlencode

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "runs/news-navigation-2026-11-03/benchmark"
CASES = ROOT / "feasibility/news-navigation-cases-v1.json"
PLAN = ROOT / "feasibility/news-navigation-plan-v1.json"
MODEL = "deepseek/deepseek-v4.1-flash-1017"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def load(path):
    return json.loads(path.read_text())


def canonical(url):
    value = urlsplit(url)
    host = value.netloc.lower().removeprefix("www.")
    query = [(k, v) for k, v in parse_qsl(value.query, keep_blank_values=True)
             if not k.lower().startswith("utm_") and k.lower() not in {"fbclid", "gclid"}]
    return urlunsplit(("", host, value.path.rstrip("/") or "/", urlencode(sorted(query)), ""))


def structured(response):
    value = response.get("result", {}).get("structuredContent")
    return value if isinstance(value, dict) else {}


def block_reason(value):
    # Examine error fields only: an article can legitimately mention CAPTCHA/HTTP403.
    errors = value.get("errors", [])
    if isinstance(errors, str):
        errors = [errors]
    if value.get("error"):
        errors = list(errors) + [str(value["error"])]
    documents = value.get("page_verification", {}).get("documents", [])
    errors = list(errors) + [str(d.get("error", "")) for d in documents]
    for error in errors:
        if re.search(r"captcha|source_challenge|source_blocked|source_policy_changed|source_disabled|rate_limited|host_disabled|document_host_disabled|HTTP (?:202|403|429)", error, re.I):
            return error
    return None


class Client:
    def __init__(self, binary, folder):
        self.folder = folder
        folder.mkdir(parents=True, exist_ok=True)
        self.serial = 0
        self.stderr = (folder / "stderr.log").open("wb")
        command = [str(binary), "mcp", "--evidence-memory", "--model", MODEL,
                   "--openrouter-key-file", os.path.expanduser("~/.config/web-search/openrouter.key"),
                   "--openrouter-proxy", "http://127.0.0.1:10808",
                   "--reranker-settings", os.path.expanduser("~/.omlx/settings.json")]
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=self.stderr, cwd=ROOT)
        self.lines = queue.Queue()

        def read():
            for line in self.proc.stdout:
                self.lines.put(line)
            self.lines.put(None)

        threading.Thread(target=read, daemon=True).start()
        try:
            self.request("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                         "clientInfo": {"name": "news-navigation-paired-check", "version": "1"}}, "initialize", 20)
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            self.request("tools/list", {}, "tools", 20)
        except Exception:
            self.close()
            raise

    def send(self, message):
        self.proc.stdin.write((json.dumps(message, ensure_ascii=False) + "\n").encode())
        self.proc.stdin.flush()

    def request(self, method, params, label, timeout=210):
        self.serial += 1
        message = {"jsonrpc": "2.0", "id": self.serial, "method": method, "params": params}
        started = time.monotonic()
        self.send(message)
        response = None
        try:
            while True:
                line = self.lines.get(timeout=max(.01, timeout - (time.monotonic() - started)))
                if line is None:
                    raise RuntimeError("MCP EOF")
                item = json.loads(line)
                if item.get("id") == self.serial:
                    response = item
                    break
        except Exception as exc:
            save(self.folder / (label + ".json"), {"request": message, "transport_error": str(exc),
                 "observed_utc": datetime.now(timezone.utc).isoformat()})
            raise
        capture = {"request": message, "response": response,
                   "observed_utc": datetime.now(timezone.utc).isoformat(),
                   "wall_ms": round((time.monotonic() - started) * 1000)}
        save(self.folder / (label + ".json"), capture)
        return capture

    def call(self, name, arguments, label):
        time.sleep(2)
        return self.request("tools/call", {"name": name, "arguments": arguments}, label)

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
            self.proc.wait(timeout=10)
        self.stderr.close()
        return self.proc.returncode


def check_protocol():
    cases = load(CASES)["cases"]
    assert len(cases) == 8 and len({c["query"] for c in cases}) == 8
    freeze = load(RUN / "protocol-freeze.json")
    for relative, wanted in freeze["sha256"].items():
        assert sha(ROOT / relative) == wanted, f"Protocol changed after freeze: {relative}"
    previous = ["новости Воронежа", "новости Воронежа сегодня 3 ноября 2026",
                "Воронеж перекрытие дорог транспорт ноябрь 2026",
                "Воронеж культурные события мероприятия 3 4 ноября 2026"]
    assert not set(previous) & {c["query"] for c in cases}
    for case in cases:
        if "сегодня" in case["query"]:
            assert case["published_from"] == case["published_to"] == "2026-11-03"
    return cases


def make_blind():
    """Output source cards without model labels; keep the join map separate."""
    rows, mapping = {}, {}
    for arm in ("baseline", "fixed"):
        for case in load(CASES)["cases"]:
            path = RUN / arm / (case["id"] + ".json")
            if not path.exists():
                continue
            value = structured(load(path).get("response", {}))
            for result in value.get("results", []):
                key = case["id"] + "\n" + canonical(result["url"])
                token = hashlib.sha256(key.encode()).hexdigest()[:16]
                row = rows.setdefault(token, {"blind_id": token, "case_id": case["id"],
                    "query": case["query"], "intent": case["intent"], "place": case["place"],
                    "topic": case["topic"], "published_from": case["published_from"],
                    "published_to": case["published_to"], "source_cards": []})
                card = {k: result.get(k) for k in ("url", "title", "snippet")}
                if card not in row["source_cards"]:
                    row["source_cards"].append(card)
                mapping.setdefault(token, []).append({"arm": arm, "case_id": case["id"],
                    "candidate_id": result.get("id"), "classification": result.get("classification"),
                    "rank": value["results"].index(result) + 1})
    save(RUN / "blind/source-cards.json", {"scope": "returned URL union; no full-web or candidate-pool gold",
         "limits": "card-only judgments are provisional; publication year is never filled from query; original source/date review required", "rows": [rows[k] for k in sorted(rows)]})
    save(RUN / "hidden/arm-map.json", mapping)
    return len(rows)


def live():
    cases = check_protocol()
    assert datetime.now(timezone(timedelta(hours=3))).date().isoformat() == "2026-11-03", "Frozen today cases require their original Moscow calendar date"
    binaries = {"baseline": ROOT / "runs/news-navigation-2026-11-03/baseline-web-search",
                "fixed": ROOT / "target/release/web-search"}
    assert all(p.is_file() for p in binaries.values())
    assert not (RUN / "execution.json").exists(), "Never overwrite or silently retry an existing live series"
    for arm in binaries:
        assert not (RUN / arm).exists(), f"Arm folder already exists: {arm}"
    code = {str(p.relative_to(ROOT)): sha(p) for p in (ROOT / "src").glob("*.rs")}
    freeze = {"created_utc": datetime.now(timezone.utc).isoformat(), "source_sha256": code,
              "binary_sha256": {arm: sha(path) for arm, path in binaries.items()},
              "protocol_sha256": load(RUN / "protocol-freeze.json")["sha256"]}
    save(RUN / "code-freeze.json", freeze)
    clients, captures = {}, []
    manifest = {"status": "running", "started_utc": datetime.now(timezone.utc).isoformat(),
                "model": MODEL, "reranker": "Qwen3-Reranker-0.6B-4bit", "evidence_memory": True,
                "protocol": "feasibility/news-navigation-plan-v1.json", "cases": cases,
                "input_equality": "live sources not frozen or guaranteed equal", "calls": []}
    save(RUN / "execution.json", manifest)
    stop = None
    try:
        for arm, binary in binaries.items():
            clients[arm] = Client(binary, RUN / arm)
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
                    stop = block_reason(value)
                manifest["calls"].append(entry)
                save(RUN / "execution.json", manifest)
                print(json.dumps(entry, ensure_ascii=False), flush=True)
        # Optional original reads use the same fixed process and do not restore blocked hosts.
        direct_urls, other_urls = [], []
        for arm, case, value in captures:
            for result in value.get("results", []):
                target = direct_urls if result.get("classification") == "direct" else other_urls
                if result["url"] not in target:
                    target.append(result["url"])
        wanted = (direct_urls + [url for url in other_urls if url not in direct_urls])[:24]
        manifest["validation"] = {"known_direct_urls": len(direct_urls), "requested_unique_cap": 24,
            "direct_over_cap": max(0, len(direct_urls) - 24), "attempted": [], "stop": stop}
        if not stop:
            for ordinal in range(0, len(wanted), 8):
                batch = wanted[ordinal:ordinal+8]
                capture = clients["fixed"].call("extract", {"urls": batch}, f"validation-{ordinal//8+1:02}")
                value = structured(capture["response"])
                manifest["validation"]["attempted"].extend(batch)
                stop = block_reason({"error": value.get("error"), "page_verification": {"documents": value.get("documents", [])}})
                if stop:
                    manifest["validation"]["stop"] = stop
                    break
        manifest["blind_unique_rows"] = make_blind()
        manifest["status"] = "completed" if len(captures) == 16 else "partial_block"
    except Exception as exc:
        manifest["status"] = "transport_or_environment_failure"
        manifest["failure"] = str(exc)
        raise
    finally:
        manifest["exit_codes"] = {arm: client.close() for arm, client in clients.items()}
        manifest["finished_utc"] = datetime.now(timezone.utc).isoformat()
        manifest["code_unchanged"] = all(sha(ROOT / path) == wanted for path, wanted in code.items())
        manifest["binaries_unchanged"] = all(sha(binaries[arm]) == wanted for arm, wanted in freeze["binary_sha256"].items())
        manifest["protocol_unchanged"] = all(sha(ROOT / path) == wanted for path, wanted in freeze["protocol_sha256"].items())
        save(RUN / "execution.json", manifest)


def self_check():
    cases = check_protocol()
    assert canonical("https://www.example.org/news/a/?utm_source=x&b=2#hello") == "//example.org/news/a?b=2"
    assert block_reason({"errors": ["naver: HTTP 403"]})
    assert block_reason({"results": [{"snippet": "CAPTCHA in news"}], "errors": []}) is None
    assert block_reason({"page_verification": {"documents": [{"error": "host_disabled: captcha"}]}})
    assert not block_reason({"errors": ["C001: fetch_timeout"]})
    assert structured({"result": {"structuredContent": {"status": "ok"}}}) == {"status": "ok"}
    assert structured({"error": {"message": "bad"}}) == {}
    print(json.dumps({"status": "passed", "cases": len(cases), "network": False}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--blind", action="store_true")
    parser.add_argument("--live", action="store_true")
    args = parser.parse_args()
    if args.live:
        live()
    elif args.blind:
        print(json.dumps({"unique_source_rows": make_blind()}))
    else:
        self_check()
