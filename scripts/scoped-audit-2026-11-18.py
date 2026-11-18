#!/usr/bin/env python3
"""Bounded new saved-material audit campaign. No source HTTP or failed-call retries."""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/scoped-audit-2026-11-18'
TEST = 'research::tests::scope_audit_experiment::scope_saved_audit_experiment'


def read(path):
    return json.loads(path.read_text())


def save(path, value):
    with path.open('x') as target:
        json.dump(value, target, ensure_ascii=False, indent=2)
        target.write('\n')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fixture(case, source):
    text, base = case['source_context'], case['source_context_start']
    assert hashlib.sha256(text.encode()).hexdigest() == case['source_context_sha256']
    assert len(case['own_quotes']) == 1
    witness = case['own_quotes'][0]
    a, b = witness['start'] - base, witness['end'] - base
    assert 0 <= a < b <= len(text) and text[a:b] == witness['quote']
    spans = [(a, b), (max(0, a - 1200), a), (b, min(len(text), b + 1200))]
    chunks = [{'id': 'S1P' + str(i + 1), 'source_id': 'S1',
               'document_version': source['document_version'], 'start': base + x,
               'end': base + y, 'text': text[x:y]} for i, (x, y) in enumerate(spans) if x < y]
    assert all(len(c['text']) <= 1200 for c in chunks)
    return {'context': {'question': case['question'],
            'facets': ['Support the frozen claim within the original question scope'],
            'sources': [{'id': 'S1', 'url_preview': case['source_url'] or 'https://example.org/synthetic/' + case['id'],
                         'title': source.get('title', ''), 'publication': source.get('publication'), 'eligible': True}],
            'chunks': chunks, 'claims': [{'id': 'A1', 'claim': {
                'text': case['claim'], 'kind': 'fact', 'rationale': '', 'facets': [0],
                'evidence': [{'chunk_id': 'S1P1', 'quote': witness['quote']}]}}]}}


def invoke(binary, path, output, arm):
    env = dict(os.environ, SCOPE_AUDIT_INPUT=str(path.resolve()),
               SCOPE_AUDIT_OUTPUT=str(output.resolve()), SCOPE_AUDIT_ARM=arm)
    with (output.parent / (output.name + '.log')).open('x') as log:
        return subprocess.run([str(binary.resolve()), TEST, '--exact', '--ignored', '--nocapture'],
                              env=env, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=105).returncode


def prepare(binary):
    assert not (RUN / 'freeze.json').exists(), 'Already frozen'
    sources = {}
    previous = read(ROOT / 'feasibility/search-repair-results-2026-11-18.json')
    for cohort in previous['cohorts'].values():
        for row in cohort['rows']:
            for source in row['value'].get('sources', []):
                sources[(source['url'], source['text_hash'])] = source
    prepared = RUN / 'prepared'
    prepared.mkdir()
    for case in read(RUN / 'inputs.json')['cases']:
        source = sources.get((case['source_url'], case['full_source_text_sha256']))
        assert source is not None or case['synthetic'], 'Missing historical source metadata'
        if source is None:
            source = {'document_version': 'synthetic-' + case['full_source_text_sha256'],
                      'title': 'Fictional API specification', 'publication': None}
        path = prepared / (case['id'] + '.json')
        value = fixture(case, source)
        save(path, value)
        code = invoke(binary, path, prepared / (case['id'] + '-prepare'), 'prepare')
        assert code == 0, f'Offline prepare exit={code} for {case["id"]}'
        generated = read(prepared / (case['id'] + '-prepare/prepared-context.json'))
        value['context_text'] = generated['context_text']
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    (RUN / 'bin').mkdir()
    shutil.copy2(binary, RUN / 'bin/audit-tests')
    files = [ROOT / 'scripts/scoped-audit-2026-11-18.py', ROOT / 'src/research_scope_audit_experiment.rs',
             ROOT / 'src/research_tests.rs', ROOT / 'src/research.rs',
             ROOT / 'feasibility/scoped-audit-cases-2026-11-18.json',
             RUN / 'inputs.json', RUN / 'gold-review.json', RUN / 'bin/audit-tests',
             *sorted(prepared.glob('*.json'))]
    ids = [c['id'] for c in read(RUN / 'inputs.json')['cases']]
    ordered = [item for pair in zip(ids[:8], ids[8:]) for item in pair]
    save(RUN / 'freeze.json', {'files': {str(p.relative_to(ROOT)): sha(p) for p in files},
         'order': ordered, 'max_calls': 32, 'wall_seconds': 600, 'model': 'deepseek/deepseek-v4.1-flash-1017',
         'limits': {'source_http': 0, 'retries': 0, 'per_call_seconds': 90, 'max_output_tokens': 3072},
         'policy': 'New authorized campaign; old429campaign not resumed. Stop first failed call. Both arms identical original context, no quote expansion or claim rewrite.'})
    print('Frozen', len(ids), 'prepared contexts; no model calls')


def run():
    freeze = read(RUN / 'freeze.json')
    for path, digest in freeze['files'].items():
        assert sha(ROOT / path) == digest, 'Freeze mismatch: ' + path
    assert not (RUN / 'stop.json').exists() and not (RUN / 'summary.json').exists()
    (RUN / 'attempts').mkdir()
    started = time.monotonic()
    attempts = []
    stop = None
    for index, case in enumerate(freeze['order']):
        for arm in (['baseline', 'candidate'] if index % 2 == 0 else ['candidate', 'baseline']):
            if len(attempts) >= freeze['max_calls'] or time.monotonic() - started >= freeze['wall_seconds'] - 108:
                stop = {'reason': 'campaign_budget', 'next_case': case, 'next_arm': arm}
                break
            time.sleep(3)
            output = RUN / 'attempts' / (case + '-' + arm)
            begin = time.monotonic()
            try:
                code = invoke(RUN / 'bin/audit-tests', RUN / 'prepared' / (case + '.json'), output, arm)
                row = {'case': case, 'arm': arm, 'exit_code': code,
                       'elapsed_ms': round((time.monotonic() - begin) * 1000)}
                if code != 0:
                    row['error'] = read(output / 'error.json') if (output / 'error.json').exists() else {'status': 'invalid_or_failed_audit'}
            except subprocess.TimeoutExpired:
                row = {'case': case, 'arm': arm, 'error': {'status': 'runner_timeout'}}
            attempts.append(row)
            print(json.dumps(row), flush=True)
            if row.get('error') or not (output / 'result.json').exists():
                stop = {'reason': 'first_failed_attempt_no_retry', 'attempt': row}
                break
        if stop:
            break
    if stop:
        save(RUN / 'stop.json', stop)
    save(RUN / 'summary.json', {'planned': 32, 'attempts': attempts, 'stop': stop,
                              'source_http_calls': 0, 'retries': 0})


def self_check():
    case = {'source_context': '😀 If ready, returns bytes.', 'source_context_start': 30,
            'own_quotes': [{'start': 42, 'end': 55, 'quote': 'returns bytes'}],
            'question': 'Does the API always return bytes?', 'claim': 'It returns bytes.',
            'source_url': 'https://example.org/api'}
    case['source_context_sha256'] = hashlib.sha256(case['source_context'].encode()).hexdigest()
    source = {'document_version': 'v1'}
    value = fixture(case, source)
    assert value['context']['chunks'][0]['text'] == 'returns bytes'
    assert value['context']['chunks'][0]['start'] == 42
    case['own_quotes'][0]['quote'] = 'invented text'
    try:
        fixture(case, source)
    except AssertionError:
        print('PASS: original Unicode quote+sourcehash, bounded chunks, rejects tampering before model')
    else:
        raise AssertionError('Tampered witness accepted')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--self-check', action='store_true')
    group.add_argument('--prepare', type=pathlib.Path)
    group.add_argument('--run', action='store_true')
    args = parser.parse_args()
    if args.self_check:
        self_check()
    elif args.prepare:
        prepare(args.prepare)
    else:
        run()
