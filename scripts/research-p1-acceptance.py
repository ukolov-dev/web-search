#!/usr/bin/env python3
"""Frozen P1 paired probes using the existing MCP/Harness drivers; no refusal retries."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/research-p1-2026-11-13'
CASES = ROOT / 'feasibility/research-p1-cases-2026-11-13.json'
PROTOCOL = ROOT / 'feasibility/research-p1-protocol-2026-11-13.json'
spec = importlib.util.spec_from_file_location('research_gates', ROOT / 'scripts/research-gates-benchmark.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


def read(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def preflight():
    protocol = read(PROTOCOL)
    assert sha(CASES) == protocol['cases_sha256'], 'Frozen cases changed'
    assert sha(RUN / 'gold-preflight.json') == protocol['gold_preflight_sha256']
    cases = read(CASES)
    assert len(cases['cases']) == len({c['id'] for c in cases['cases']}) == protocol['cohorts']['fresh_questions'] + protocol['cohorts']['known_controls']
    assert sum(c['role'].startswith('new_question') for c in cases['cases']) == protocol['cohorts']['fresh_questions']
    assert len(cases['harness']) == 2
    seed, state = read(RUN / 'seed-ledger.json'), read(RUN / 'source-state.json')
    for key in ['blocked', 'document_blocks']:
        assert all(state[key].get(h) == v for h, v in seed[key].items())
    assert sha(RUN / 'bin/baseline') == protocol['baseline_binary_sha256']
    return cases


def run(arm, group):
    cases = preflight()
    binary = RUN / 'bin' / arm
    assert sha(binary) == read(RUN / (arm + '.json'))['binary_sha256']
    arm_root = RUN / arm
    arm_root.mkdir(exist_ok=True)
    selected = cases['harness' if group == 'harness' else 'cases']
    with (RUN / 'live.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        for case in selected:
            directory = arm_root / group / case['id']
            assert not directory.exists(), 'Never overwrite an attempted trial'
            assert not (RUN / 'model-refused.json').exists(), 'Model refusal persists; no retry'
            before = read(RUN / 'source-state.json')
            if all(e in before['blocked'] for e in ['seznam', 'naver']):
                raise SystemExit('Both allowed search sources unavailable; no retries')
            shutil.copyfile(RUN / 'source-state.json', arm_root / 'source-state.json')
            directory.parent.mkdir(parents=True, exist_ok=True)
            seed = {key: dict(value) for key, value in before.items()}
            time.sleep(2.1)
            try:
                if group == 'harness':
                    result = driver.shared.run_case(case, group, binary, arm_root / 'cache.sqlite', run_root=arm_root,
                                                    guard=ROOT / 'scripts/research-harness-guard.mjs')
                    driver.check_harness(case, directory, result)
                else:
                    directory.mkdir(parents=True)
                    driver.probe.RUN, driver.probe.BINARY = arm_root, binary
                    result = driver.probe.mcp_case(case, directory)
            except Exception as error:
                directory.mkdir(parents=True, exist_ok=True)
                result = {'status': 'runner_error', 'error': type(error).__name__ + ': ' + str(error)[:300]}
            after = read(arm_root / 'source-state.json')
            for key in ['blocked', 'document_blocks']:
                before[key].update(after[key])
            driver.shared.save(RUN / 'source-state.json', before)
            driver.shared.save(directory / 'ledger-after.json', before)
            driver.shared.save(directory / 'ledger-before.json', seed)
            result.update(id=case['id'], arm=arm, cohort=group, binary_sha256=sha(binary))
            captures = list((directory / ('mcp-capture' if group == 'harness' else 'capture')).glob('*-research'))
            inner_refusal = any(driver.probe.refused(read(p / 'result.json')) for p in captures if (p / 'result.json').exists())
            if inner_refusal:
                result['model_refusal'] = True
            driver.shared.save(directory / 'summary.json', result)
            print(json.dumps({k: result.get(k) for k in ['id', 'arm', 'cohort', 'status', 'tool_status', 'elapsed_ms']}, ensure_ascii=False), flush=True)
            if result.get('model_refusal') or result.get('refusal'):
                driver.shared.save(RUN / 'model-refused.json', {'id': case['id'], 'arm': arm, 'no_retry': True})
                raise SystemExit('Model refusal; stopped without retry')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['self-check', 'run'])
    parser.add_argument('--arm', choices=['baseline', 'candidate'])
    parser.add_argument('--group', choices=['cases', 'harness'], default='cases')
    parser.add_argument('--live', action='store_true')
    args = parser.parse_args()
    if args.command == 'self-check':
        preflight()
        print('PASS: frozen cohort/two Harness, baseline binary/gold hashes and cumulative refusal seed')
    else:
        assert args.arm and args.live, 'Explicit arm and live required'
        run(args.arm, args.group)
