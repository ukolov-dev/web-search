#!/usr/bin/env python3
"""One frozen development scenario through the real MCP; no LLM or source retries."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / 'feasibility/core-scenario-2026-11-05.json'

def matches(url, case):
    parsed = urlsplit(url)
    return parsed.hostname == 'developer.mozilla.org' and parsed.path.rstrip('/').endswith(case['path'])

def qualifies(doc, case):
    text = doc.get('text', '').lower()
    return doc.get('status') == 'ok' and matches(doc.get('final_url', ''), case) and all(s.lower() in text for s in case['phrases'])

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--live', action='store_true')
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--source-state', type=Path)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/web-search')
    args = parser.parse_args()
    cases = json.loads(CASES.read_text())
    if args.self_test:
        case = cases['cases'][0]
        assert matches('https://developer.mozilla.org/en-US/docs' + case['path'], case)
        assert not matches('https://developer.mozilla.org.attacker.test/en-US/docs' + case['path'], case)
        assert not matches('https://developer.mozilla.org/en-US/docs/Web/unrelated', case)
        doc = {'status': 'ok', 'final_url': 'https://developer.mozilla.org/en-US/docs' + case['path'], 'text': 'empty array returns true'}
        assert qualifies(doc, case)
        assert not qualifies({**doc, 'text': 'true but no evidence'}, case)
        assert not qualifies({**doc, 'status': 'error'}, case)
        print('core scenario URL and evidence checks: PASS')
        return
    if not (args.live and args.output and args.source_state):
        parser.error('--live, --output and --source-state are required')
    assert args.source_state.exists(), 'Use the existing refusal ledger, never start an empty one'
    ledger = json.loads(args.source_state.read_text())
    assert cases['engine'] not in ledger.get('blocked', {}), 'Selected engine is already blocked'
    args.output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('wire', ROOT / 'scripts/harness-cache-benchmark.py')
    wire = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(wire)
    report = {'scenario': cases['scenario'], 'case_sha256': hashlib.sha256(CASES.read_bytes()).hexdigest(),
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(), 'cases': [],
              'scope': cases['scope'], 'llm_calls': 0}
    with (args.output / 'stderr.log').open('w') as stderr, (args.output / 'wire.jsonl').open('w') as events:
        child = subprocess.Popen([str(args.binary.resolve()), 'mcp', '--source-state', str(args.source_state.resolve())], cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
        try:
            rpc = wire.Rpc(child, events)
            rpc.call('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'core-scenario', 'version': '1'}}, time.monotonic()+15)
            child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); child.stdin.flush()
            for case in cases['cases']:
                item = {'id': case['id'], 'query': case['query'], 'status': 'not_run'}
                report['cases'].append(item)
                result, _ = rpc.call('tools/call', {'name': 'search', 'arguments': {'engine': cases['engine'], 'query': case['query'], 'max_results': 10}}, time.monotonic()+60)
                item['search'] = result
                if result.get('isError'):
                    item['status'] = 'search_failed'
                    break
                value = result.get('structuredContent', {})
                candidates = [r for r in value.get('results', []) if matches(r['url'], case)]
                if not candidates:
                    item['status'] = 'target_not_found'
                else:
                    item['selected_url'] = candidates[0]['url']
                    result, _ = rpc.call('tools/call', {'name': 'extract', 'arguments': {'urls': [item['selected_url']]}}, time.monotonic()+60)
                    item['extract'] = result
                    documents = result.get('structuredContent', {}).get('documents', [])
                    doc = documents[0] if documents else {}
                    text = doc.get('text', '')
                    item['phrase_checks'] = {s: s.lower() in text.lower() for s in case['phrases']}
                    item['status'] = 'passed' if qualifies(doc, case) else 'extract_or_evidence_failed'
                (args.output / 'results.json').write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
                print(case['id'], item['status'], flush=True)
                time.sleep(2)
        finally:
            child.terminate()
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired: child.kill(); child.wait()
            (args.output / 'results.json').write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
    assert len(report['cases']) == len(cases['cases']) and all(c['status'] == 'passed' for c in report['cases']), 'Scenario did not pass; preserve all failures'

if __name__ == '__main__':
    main()
