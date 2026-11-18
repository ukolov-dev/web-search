#!/usr/bin/env python3
"""Frozen full native research controller comparison; never seed reference URLs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/primary-search-2026-11-18'
CASES = ROOT / 'feasibility/primary-search-cases-2026-11-18.json'
TEST = 'research::tests::primary_search_experiment::full_native_primary_search'
ORDER = [('C01', 'baseline'), ('C01', 'candidate'), ('P01', 'candidate'), ('P01', 'baseline'),
         ('P02', 'baseline'), ('P02', 'candidate'), ('P03', 'candidate'), ('P03', 'baseline'),
         ('P04', 'baseline'), ('P04', 'candidate'), ('C02', 'candidate'), ('C02', 'baseline')]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def question(case):
    return {key: case[key] for key in ['question', 'language']}


def prepare(binary, seed=None, extra_files=()):
    assert not (RUN / 'freeze.json').exists()
    cases = read(CASES)['cases']
    assert len(cases) == 6 and set(c['id'] for c in cases) == set(c for c, _ in ORDER)
    prepared = RUN / 'prepared'
    prepared.mkdir()
    for case in cases:
        save(prepared / (case['id'] + '.json'), question(case))
    if seed is None:
        seed = read(ROOT / 'feasibility/primary-draft-results-2026-11-18.json')['source_state_after']
    assert 'seznam' not in seed['blocked']
    save(RUN / 'seed-ledger.json', seed)
    save(RUN / 'source-state.json', seed)
    (RUN / 'bin').mkdir()
    stable = RUN / 'bin/research-tests'
    shutil.copy2(binary.resolve(), stable)
    listing = subprocess.run([str(stable), '--list'], capture_output=True, text=True, check=True)
    assert TEST + ': test' in listing.stdout
    files = [CASES, RUN / 'gold-review.json', RUN / 'seed-ledger.json', stable,
             Path(__file__).resolve(), ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
             *sorted(prepared.glob('*.json')), *sorted((ROOT / 'src').glob('*.rs')), *extra_files]
    save(RUN / 'freeze.json', {'files': {str(p.relative_to(ROOT)): sha(p) for p in files},
         'qwen_settings_sha256': sha(Path.home() / '.omlx/settings.json'),
         'order': ORDER, 'model': 'deepseek/deepseek-v4.1-flash-1017', 'pace_seconds': 10,
         'limits_per_trial': {'seconds': 180, 'generations': 6, 'documents': 4, 'serp': 4, 'qwen': 4},
         'limits_campaign': {'trials': 12, 'generations': 72, 'documents': 48, 'serp': 48, 'qwen': 48},
         'scope': 'Native Session.execute including planner/search/extract/Qwen/draft/audit; no seeded URL/plan/body. Shared guard/pacing for both arms. Only candidate read-order differs. No MCP transport intervention.'})
    print('Prepared12trials; no product/model calls')


def failure(value, experiment, output):
    if experiment.get('halt') or value.get('error'):
        return 'first_error'
    if value.get('stop_reason') in ['model_error', 'deadline', 'sources_unavailable', 'cancelled']:
        return 'failed_stop'
    if any(a.get('error') or (a.get('action') == 'qwen' and a.get('status') != 'ok') for a in value.get('actions', [])):
        return 'source_or_qwen_error'
    if any(c.get('status') != 'returned' for c in value.get('model_calls', [])):
        return 'model_call_error'
    if list(output.glob('request-rejected-*.json')):
        return 'model_refused'
    return None


def assert_ledger_extends(before, after):
    for category in ['blocked', 'document_blocks', 'document_url_blocks', 'legacy_timeout_host_exclusions']:
        assert all(after.get(category, {}).get(key) == value for key, value in before.get(category, {}).items()), 'Source state lost: ' + category


def run():
    frozen = read(RUN / 'freeze.json')
    assert frozen['order'] == [list(pair) for pair in ORDER]
    for name, digest in frozen['files'].items():
        assert sha(ROOT / name) == digest, 'Freeze mismatch: ' + name
    assert sha(Path.home() / '.omlx/settings.json') == frozen['qwen_settings_sha256']
    attempts = RUN / 'attempts'
    attempts.mkdir()  # No resume, even following a crash or incomplete attempt.
    rows = []
    totals = dict.fromkeys(['generations', 'documents', 'serp', 'qwen'], 0)
    stop = None
    seed = read(RUN / 'seed-ledger.json')
    for index, (case, arm) in enumerate(ORDER):
        before = read(RUN / 'source-state.json')
        assert_ledger_extends(seed, before)
        if 'seznam' in before['blocked']:
            stop = 'source_already_blocked'
            break
        if index:
            time.sleep(10)
        name = case + '-' + arm
        output = attempts / name
        row = {'case': case, 'arm': arm}
        save(attempts / (name + '-ledger-before.json'), before)
        env = dict(os.environ, PRIMARY_SEARCH_INPUT=str((RUN / 'prepared' / (case + '.json')).resolve()),
                   PRIMARY_SEARCH_OUTPUT=str(output.resolve()), PRIMARY_SEARCH_ARM=arm,
                   PRIMARY_SEARCH_STATE=str((RUN / 'source-state.json').resolve()))
        started = time.monotonic()
        try:
            with (attempts / (name + '.log')).open('x') as log:
                child = subprocess.run([str(RUN / 'bin/research-tests'), TEST, '--exact', '--ignored', '--nocapture'],
                                       cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=200)
            row['exit_code'] = child.returncode
            after = read(RUN / 'source-state.json')
            assert_ledger_extends(before, after)
            save(attempts / (name + '-ledger-after.json'), after)
            row['model_transport_attempt_intents'] = len(list(output.glob('request-attempted-*.json')))
            row['model_response_captures'] = len(list(output.glob('model-*.json')))
            value = read(output / 'result.json')
            experiment = read(output / 'experiment.json')
            used = value['budget']['used']
            for key in totals:
                assert type(used[key]) is int and 0 <= used[key] <= frozen['limits_per_trial'][key]
                totals[key] += used[key]
            row.update(status=value['status'], stop_reason=value['stop_reason'], used=used,
                       error=failure(value, experiment, output))
            if child.returncode:
                row['error'] = row['error'] or 'nonzero_exit'
        except subprocess.TimeoutExpired:
            row['error'] = 'process_timeout_unknown_usage'
        except Exception as error:
            row['error'] = 'runner_exception:' + type(error).__name__
        row['elapsed_ms'] = round((time.monotonic() - started) * 1000)
        rows.append(row)
        save(attempts / (name + '-attempt.json'), row)
        print(json.dumps(row), flush=True)
        if row.get('error'):
            stop = 'first_error_no_retry'
            break
    save(RUN / 'summary.json', {'planned': 12, 'attempts': rows, 'not_run': ORDER[len(rows):],
         'stop': stop, 'retries': 0, 'recorded_budget_used': totals,
         'unrecorded_usage_possible': any('used' not in row for row in rows)})


def self_check():
    assert question({'question': 'Вопрос😀', 'language': 'ru', 'url': 'NEVER', 'gold': 'FORBIDDEN'}) == {'question': 'Вопрос😀', 'language': 'ru'}
    assert len(ORDER) == 12 and len(set(ORDER)) == 12
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)
        assert failure({}, {'halt': 'HTTP429'}, path) == 'first_error'
        assert failure({'error': 'source_timeout'}, {}, path) == 'first_error'
        assert failure({'actions': [{'action': 'qwen', 'status': 'failed_lexical_fallback'}]}, {}, path) == 'source_or_qwen_error'
        assert failure({'stop_reason': 'budget_exhausted'}, {}, path) is None
        assert failure({'model_calls': [{'status': 'error'}]}, {}, path) == 'model_call_error'
        save(path / 'new.json', {})
        try:
            save(path / 'new.json', {})
        except FileExistsError:
            pass
        else:
            raise AssertionError('overwrite accepted')
    print('PASS: question-only inputs, pairedorder, failure-stop, no overwrite')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--prepare', type=Path)
    mode.add_argument('--run', action='store_true')
    mode.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    if args.prepare:
        prepare(args.prepare)
    elif args.run:
        run()
    else:
        self_check()
