#!/usr/bin/env python3
"""Paired frozen research probes; reuse the existing MCP and real Harness drivers."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/research-gates-2026-11-06'
CASES = ROOT / 'feasibility/research-gates-cases-2026-11-06.json'
spec = importlib.util.spec_from_file_location('luna_probe', ROOT / 'scripts/luna-seven-benchmark.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
shared = probe.shared


def load_cases(group='new'):
    if group == 'final':
        path=ROOT/'feasibility/research-gates-final-cases-2026-11-06.json'
        assert hashlib.sha256(path.read_bytes()).hexdigest() == json.loads((RUN/'final-freeze.json').read_text())['sha256']
        return json.loads(path.read_text())
    if group == 'confirm':
        path=ROOT/'feasibility/research-gates-confirm-cases-2026-11-06.json'
        assert hashlib.sha256(path.read_bytes()).hexdigest() == json.loads((RUN/'confirmation-freeze.json').read_text())['sha256']
        return json.loads(path.read_text())
    snapshot = json.loads((RUN / 'baseline.json').read_text())
    assert hashlib.sha256(CASES.read_bytes()).hexdigest() == snapshot['cases_sha256'], 'Frozen cases changed'
    return json.loads(CASES.read_text())


def check_harness(case, directory, result):
    captures = list((directory / 'mcp-capture').glob('*-research'))
    values = [json.loads((c / 'result.json').read_text()) for c in captures]
    result['model_refusal'] = any(probe.refused(v) for v in values)
    assert len(captures) == 1, 'Exactly one actual research invocation required'
    assert not result['model_refusal'], 'Inner model refused; do not retry'
    first = json.loads((captures[0] / 'model-01-plan.json').read_text())
    context = json.loads(first['input']['messages'][-1]['content'])
    assert context['question'] == case['prompt'].split('\nВызови research',1)[0], 'Harness changed the frozen question'
    result['provenance_checks'] = [probe.probe.check(v,c) for v,c in zip(values,captures)]


def run(arm, group):
    fixture = load_cases(group)
    binary = RUN / 'bin' / arm
    snapshot = json.loads((RUN / f'{arm}.json').read_text())
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == snapshot['binary_sha256']
    if group == 'old':
        cases = [c for c in json.loads((ROOT / 'feasibility/luna-seven-cases-2026-11-06.json').read_text())['cases'] if c['id'] in ('R01', 'R03')]
    else:
        cases = fixture['harness' if group == 'harness' else 'cases']
    probe.RUN, probe.BINARY = RUN, binary
    with (RUN / 'live.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        for case in cases:
            directory = RUN / arm / group / case['id']
            assert not directory.exists(), 'Never overwrite a trial'
            state = json.loads((RUN / 'source-state.json').read_text())
            assert not (RUN / 'model-refused.json').exists(), 'Preserve model refusal'
            assert not all(e in state['blocked'] for e in ('seznam', 'naver')), 'Sources unavailable; do not retry'
            time.sleep(2.1)
            result = {}
            try:
                if group == 'harness':
                    result = shared.run_case(case, arm+'/'+group, binary, RUN / 'cache.sqlite', run_root=RUN, guard=ROOT / 'scripts/research-harness-guard.mjs')
                    check_harness(case,directory,result)
                else:
                    directory.mkdir(parents=True)
                    result = probe.mcp_case(case, directory)
            except Exception as error:
                result.update(status='runner_error',error=type(error).__name__+': '+str(error)[:300])
            shared.save(directory / 'ledger-before.json', state)
            shared.save(directory / 'ledger-after.json', json.loads((RUN / 'source-state.json').read_text()))
            result.update(id=case['id'], arm=arm, group=group, binary_sha256=snapshot['binary_sha256'])
            shared.save(directory / 'summary.json', result)
            if result.get('model_refusal') or result.get('refusal'):
                shared.save(RUN / 'model-refused.json', {'case': case['id'], 'arm': arm, 'no_retry': True})
            print(json.dumps({k: result.get(k) for k in ('id','arm','group','status','elapsed_ms','tool_status')}, ensure_ascii=False), flush=True)


def self_check():
    x = load_cases()
    assert len(x['cases']) == len({c['id'] for c in x['cases']}) == 12
    assert [sum(c['group'] == g for c in x['cases']) for g in ('RFC','technical','scope')] == [4,4,4]
    assert len(x['harness']) == 2
    for group in ['confirm','final']:
        cohort=load_cases(group)['cases']
        assert len(cohort)==len({c['id'] for c in cohort})==12
        assert [sum(c['group']==g for c in cohort) for g in ('RFC','technical','scope')]==[4,4,4]
    assert all(c['gold_aspects'] and c['references'] for c in x['cases']+x['harness'])
    state = json.loads((RUN / 'source-state.json').read_text())
    seed = json.loads((RUN / 'seed-ledger.json').read_text())
    assert all(k in state['blocked'] for k in seed['blocked'])
    assert all(k in state['document_blocks'] for k in seed['document_blocks'])
    assert probe.refused({'error': 'OpenRouter HTTP 403 Forbidden'})
    assert not probe.refused({'error': 'source_blocked HTTP 403'})
    with tempfile.TemporaryDirectory() as tmp:
        directory=Path(tmp);capture=directory/'mcp-capture/0000-research';capture.mkdir(parents=True)
        shared.save(capture/'result.json',{'error':'OpenRouter HTTP 403 Forbidden'})
        result={}
        try:
            check_harness({'prompt':'q'},directory,result)
            assert False,'An inner refusal must fail before attempting to read the absent plan'
        except AssertionError as error:
            assert result['model_refusal'] and 'refused' in str(error)
    print('PASS: frozen NEW12 4+4+4, two Harness tasks, refusal state and provenance driver')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['self-check','run'])
    parser.add_argument('--arm', choices=['baseline','candidate','candidate-v2','candidate-v3','candidate-v4'])
    parser.add_argument('--group', choices=['old','new','harness','confirm','final'], default='new')
    parser.add_argument('--live', action='store_true')
    args = parser.parse_args()
    if args.command == 'self-check':
        self_check()
    else:
        assert args.arm and args.live, 'Explicit arm and --live required'
        run(args.arm, args.group)
