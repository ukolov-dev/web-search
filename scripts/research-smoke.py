#!/usr/bin/env python3
"""Explicit live MCP development probe; never overwrites a trial or resets source refusals."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import shutil
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/research-mvp-2026-11-05'
MODEL = 'deepseek/deepseek-v4.1-flash-1017'
spec = importlib.util.spec_from_file_location('harness_probe', ROOT / 'scripts/harness-cache-benchmark.py')
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)


def check(value, capture):
    assert len(json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()) <= 32768
    used = value['budget']['used']
    for name, limit in {'forms': 2, 'serp': 4, 'documents': 4, 'qwen': 4, 'generations': 6}.items():
        assert used[name] <= limit
    originals = {}
    for path in capture.glob('document-*.json'):
        doc = json.loads(path.read_text())
        originals[(doc['url'], hashlib.sha256(doc['text'].encode()).hexdigest())] = doc['text']
    sources = {s['id']: s for s in value['sources']}
    count = 0
    for claim in value['claims']:
        assert value['answer'][claim['answer_start']:claim['answer_end']] == claim['text']
        assert claim['evidence']
        for witness in claim['evidence']:
            source = sources[witness['source_id']]
            text = originals[(source['url'], source['text_hash'])]
            assert witness['document_version'] == source['document_version']
            assert text[witness['start']:witness['end']] == witness['quote']
            assert source['eligible']
            count += 1
    return count


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--live', action='store_true')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/web-search')
    parser.add_argument('--case', choices=['R01', 'R02', 'R03', 'R04', 'R05', 'R06'])
    args = parser.parse_args()
    assert args.live, 'Explicit --live required'
    confirmation = args.case in ['R05', 'R06']
    cases_path = ROOT / ('feasibility/research-mvp-confirm-cases-2026-11-05.json' if confirmation else 'feasibility/research-mvp-cases-2026-11-05.json')
    freeze = json.loads((RUN / ('confirm-case-freeze.json' if confirmation else 'case-freeze.json')).read_text())
    assert hashlib.sha256(cases_path.read_bytes()).hexdigest() == freeze['sha256']
    cases = json.loads(cases_path.read_text())['cases']
    cases = [c for c in cases if not args.case or c['id'] == args.case]
    # All trials use the same mutable refusal ledger, including across process restarts.
    for case in cases:
        directory = RUN / ('confirmation' if confirmation else 'primary') / case['id']
        directory.mkdir(parents=True, exist_ok=False)
        shutil.copyfile(RUN / 'source-state.json', directory / 'ledger-before.json')
        shared.save(directory / 'binary.json', {'sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()})
        capture = directory / 'capture'
        command = [str(args.binary.resolve()), 'mcp', '--openrouter-key-file', str(Path.home() / '.config/web-search/openrouter.key'),
                   '--openrouter-proxy', 'http://127.0.0.1:10808', '--model', MODEL, '--evidence-memory',
                   '--reranker-settings', str(Path.home() / '.omlx/settings.json'), '--source-state', str(RUN / 'source-state.json'),
                   '--capture-dir', str(capture), '--cache-db', str(RUN / 'cache.sqlite')]
        started = time.monotonic()
        with (directory / 'stderr.log').open('w') as stderr, (directory / 'wire.jsonl').open('w') as wire:
            child = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
            try:
                rpc = shared.Rpc(child, wire)
                info, _ = rpc.call('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'research-dev-probe', 'version': '1'}}, time.monotonic()+15)
                child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); child.stdin.flush()
                tools, _ = rpc.call('tools/list', {}, time.monotonic()+10)
                assert sorted(t['name'] for t in tools['tools']) == ['discover', 'extract', 'ranked_search', 'research', 'search']
                shared.save(directory / 'discovery.json', {'initialize': info, 'tools': tools})
                result, _ = rpc.call('tools/call', {'name': 'research', 'arguments': {'question': case['question'], 'language': 'ru'}}, time.monotonic()+200)
                shared.save(directory / 'tool-result.json', result)
                value = result['structuredContent']
                captures = list(capture.glob('*-research'))
                assert len(captures) == 1
                witnesses = check(value, captures[0])
                summary = {'id': case['id'], 'status': value['status'], 'stop_reason': value['stop_reason'],
                           'elapsed_ms': round((time.monotonic()-started)*1000), 'budget': value['budget']['used'],
                           'claims': len(value['claims']), 'witnesses_checked': witnesses, 'sources': len(value['sources']),
                           'public_bytes': len(json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()),
                           'text_bytes': sum(len(c.get('text', '').encode()) for c in result.get('content', [])),
                           'wire_bytes': len(json.dumps(result, ensure_ascii=False).encode()),
                           'model_cost_usd_known': sum((c.get('usage') or {}).get('cost', 0) or 0 for c in value['model_calls']),
                           'unknown_cost_attempts': sum(c.get('cost_known') is False for c in value['model_calls']), 'cache': value['cache']}
                shared.save(directory / 'summary.json', summary)
                shutil.copyfile(RUN / 'source-state.json', directory / 'ledger-after.json')
                print(json.dumps(summary, ensure_ascii=False), flush=True)
            finally:
                child.terminate()
                try: child.wait(timeout=5)
                except subprocess.TimeoutExpired: child.kill(); child.wait()


if __name__ == '__main__':
    main()
