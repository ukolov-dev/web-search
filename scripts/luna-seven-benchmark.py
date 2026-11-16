#!/usr/bin/env python3
"""Frozen, serialized tests shared by seven Luna assessors. Product runtime is unchanged."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/luna-seven-2026-11-06'
CASES = ROOT / 'feasibility/luna-seven-cases-2026-11-06.json'
BINARY = RUN / 'bin/web-search'
spec = importlib.util.spec_from_file_location('research_probe', ROOT / 'scripts/research-smoke.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
shared = probe.shared


def corpus_check(value, directory):
    """Mechanically verify provenance without calling it semantic support."""
    captures = list((directory / 'capture').glob('*-research'))
    if not captures:
        return {'claim_ranges': 0, 'witnesses': 0}
    assert len(captures) == 1
    witnesses = probe.check(value, captures[0])
    return {'claim_ranges': len(value['claims']), 'witnesses': witnesses}


def refused(value):
    message = json.dumps(value.get('error'), ensure_ascii=False).lower()
    return any(f'{prefix} http {code}' in message for prefix in ['openrouter', 'model preflight'] for code in [401, 403, 429])


def mcp_case(case, directory):
    args = [str(BINARY), 'mcp', '--openrouter-key-file', str(Path.home() / '.config/web-search/openrouter.key'),
            '--openrouter-proxy', 'http://127.0.0.1:10808', '--model', probe.MODEL, '--evidence-memory',
            '--reranker-settings', str(Path.home() / '.omlx/settings.json'), '--source-state', str(RUN / 'source-state.json'),
            '--cache-db', str(RUN / 'cache.sqlite'), '--capture-dir', str(directory / 'capture')]
    params = {'question': case['question'], 'language': 'ru'} if case['tool'] == 'research' else {'query': case['query'], 'max_results': 5}
    if case['tool'] == 'search': params['engine'] = 'seznam'
    started = time.monotonic()
    with (directory / 'stderr.log').open('w') as err, (directory / 'wire.jsonl').open('w') as wire:
        child = subprocess.Popen(args, cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err)
        try:
            rpc = shared.Rpc(child, wire)
            rpc.call('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'luna-seven', 'version': '1'}}, time.monotonic()+15)
            child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); child.stdin.flush()
            tools, _ = rpc.call('tools/list', {}, time.monotonic()+10)
            assert sorted(t['name'] for t in tools['tools']) == ['discover', 'extract', 'ranked_search', 'research', 'search']
            shared.save(directory / 'discovery.json', tools)
            tool, _ = rpc.call('tools/call', {'name': case['tool'], 'arguments': params}, time.monotonic()+210)
            shared.save(directory / 'tool-result.json', tool)
            value = tool['structuredContent']
            checks = corpus_check(value, directory) if case['tool'] == 'research' else {}
            return {'status': 'completed', 'tool_status': value.get('status'), 'stop_reason': value.get('stop_reason'),
                    'elapsed_ms': round((time.monotonic()-started)*1000), 'is_error': tool.get('isError'),
                    'public_bytes': len(json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode()),
                    'text_bytes': sum(len(c.get('text', '').encode()) for c in tool.get('content', [])),
                    'cache': value.get('cache'), 'budget': value.get('budget'), 'provenance_checks': checks, 'model_refusal': refused(value)}
        finally:
            child.terminate()
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired: child.kill(); child.wait()


def run(case):
    snapshot = json.loads((RUN / 'snapshot.json').read_text())
    assert hashlib.sha256(BINARY.read_bytes()).hexdigest() == snapshot['binary_sha256']
    assert hashlib.sha256(CASES.read_bytes()).hexdigest() == json.loads((RUN / 'case-freeze.json').read_text())['sha256']
    directory = RUN / case['agent'] / 'episodes' / case['id']
    directory.parent.mkdir(exist_ok=True)
    assert not directory.exists(), 'Never overwrite a trial'
    queued = time.monotonic()
    # A single lock protects pacing, cache ownership, refusal state and provider request admission.
    with (RUN / 'live.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        time.sleep(2.1)
        state = json.loads((RUN / 'source-state.json').read_text())
        model_blocked = (RUN / 'model-refused.json').exists() and case['tool'] != 'search'
        sources_blocked = all(e in state['blocked'] for e in ['seznam', 'naver'])
        if model_blocked or sources_blocked:
            directory.mkdir()
            result = {'status': 'not_run', 'reason': 'model_refusal' if model_blocked else 'sources_unavailable'}
        else:
            # The shared Harness driver creates its own directory and captures the actual SDK output.
            if case['tool'] == 'harness':
                h = shared.run_case(case, case['agent']+'/episodes', BINARY, RUN / 'cache.sqlite', run_root=RUN, guard=ROOT / 'scripts/luna-seven-harness-guard.mjs')
                result = {'status': h['status'], 'elapsed_ms': h.get('task_elapsed_ms'), 'model_refusal': h.get('refusal', False)}
                captures = list((directory / 'mcp-capture').glob('*-research'))
                for capture in captures:
                    value = json.loads((capture / 'result.json').read_text())
                    result['provenance_checks'] = {'witnesses': probe.check(value, capture), 'claim_ranges': len(value['claims'])}
                    result['model_refusal'] = result['model_refusal'] or refused(value)
            else:
                directory.mkdir()
                try: result = mcp_case(case, directory)
                except Exception as error: result = {'status': 'runner_error', 'error': type(error).__name__+': '+str(error)[:300]}
            if result.get('model_refusal'): shared.save(RUN / 'model-refused.json', {'case': case['id'], 'no_retry': True})
        result.update(id=case['id'], agent=case['agent'], tool=case['tool'], binary_sha256=snapshot['binary_sha256'], queue_ms=round((time.monotonic()-queued)*1000)-int(result.get('elapsed_ms') or 0))
        shared.save(directory / 'summary.json', result)
        shared.save(directory / 'case.json', case)
        shared.save(directory / 'ledger-after.json', json.loads((RUN / 'source-state.json').read_text()))
        print(json.dumps(result, ensure_ascii=False), flush=True)


def self_check():
    cases = json.loads(CASES.read_text())['cases']
    assert len(cases) == 14 and len({c['id'] for c in cases}) == 14
    assert set(c['agent'] for c in cases) == {'L5', 'L6', 'L7'}
    assert {c['tool'] for c in cases} == {'search', 'ranked_search', 'research', 'harness'}
    assert refused({'error': 'OpenRouter HTTP 403 Forbidden'})
    assert refused({'error': {'code': 'model_unavailable', 'message': 'model preflight HTTP 429'}})
    assert not refused({'error': 'source_blocked: HTTP 403'})
    assert not refused({'answer': 'A source says OpenRouter HTTP 403'})
    assert json.loads((RUN / 'seed-ledger.json').read_text())['document_blocks']['redis.io']
    assert hashlib.sha256(CASES.read_bytes()).hexdigest() == json.loads((RUN / 'case-freeze.json').read_text())['sha256']
    import inspect
    parameters = inspect.signature(shared.run_case).parameters
    assert parameters['run_root'].default is None and parameters['guard'].default is None
    assert 'node = RUN /' in inspect.getsource(shared.run_case), 'SDK runtime path must remain the previous pinned installation'
    subprocess.run(['node', '--input-type=module', '-e', '''
import assert from 'node:assert/strict';
import Guard from './scripts/luna-seven-harness-guard.mjs';
const names = ['search','discover','extract','ranked_search','research'].map(n=>'mcp__web_search__'+n);
const visible = new Map([...names,'native_shell'].map(n=>[n,{}]));
let check, dispose;
const tools = {view(){return {visible}},guard(fn){check=fn;return ()=>{}}};
const ctx = {tools,systemPrompt:{section(){}},on(_,fn){dispose=fn}};
new Guard(ctx,{ready:'runs/luna-seven-2026-11-06/guard-ready.json'});
try {
  assert.equal(tools.view().visible.size,5);
  const agent={};
  assert.equal(check({name:'native_shell',agent}), 'LUNA_TEST_SEARCH_ONLY');
  assert.equal(check({name:names[0],arguments:{engine:'naver'},agent}), 'LUNA_TEST_SOURCE_NOT_ALLOWED');
  assert.equal(check({name:names[4],agent}), undefined);
  assert.equal(check({name:names[4],agent}), 'LUNA_TEST_ONE_RESEARCH');
  assert.equal(check({name:names[0],arguments:{engine:'seznam'},agent}), undefined);
  assert.equal(check({name:names[2],agent}), 'LUNA_TEST_THREE_TOOLS');
} finally {dispose()}
assert.equal(tools.view().visible.size,6);
'''], cwd=ROOT, check=True)
    print('PASS: frozen14 cases, model/source refusal distinction, bounded shared driver defaults')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('self-check')
    live = sub.add_parser('run')
    live.add_argument('--case', required=True)
    live.add_argument('--live', action='store_true')
    args = parser.parse_args()
    if args.command == 'self-check': self_check()
    else:
        assert args.live, 'Explicit --live required'
        case = next(c for c in json.loads(CASES.read_text())['cases'] if c['id'] == args.case)
        run(case)
