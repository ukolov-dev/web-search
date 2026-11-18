#!/usr/bin/env python3
"""Known-primary-source component, never a retrieval or runtime qualification."""
import copy
import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import time
import argparse

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/primary-evidence-2026-11-18'
QWEN_TEST = 'research::tests::scope_audit_experiment::primary_saved_qwen'


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


shared = module('previous_runner', 'scripts/scoped-audit-2026-11-18.py')
restore = module('fixture_restorer', 'scripts/primary-evidence-fixture.py').restore
read, save, sha = shared.read, shared.save, shared.sha


def add_primary(value, document, ranges):
    value = copy.deepcopy(value)
    context = value['context']
    assert all(s['id'] != 'S3' for s in context['sources'])
    text = document['text']
    assert hashlib.sha256(text.encode()).hexdigest() == document['text_hash']
    version = hashlib.sha256(json.dumps(['primary-saved-extract-v1', document['final_url'], document['text_hash']], separators=(',', ':')).encode()).hexdigest()
    context['sources'].append({'id': 'S3', 'url_preview': document['final_url'],
                              'title': document.get('title', ''), 'publication': None, 'eligible': True})
    assert 1 <= len(ranges) <= 3
    for i, (a, b) in enumerate(ranges):
        raw = text.encode()
        selected = raw[a:b].decode()
        assert 0 <= a < b <= len(raw) and 1 <= len(selected) <= 1200
        start = len(raw[:a].decode())
        context['chunks'].append({'id': 'S3P' + str(i + 1), 'source_id': 'S3',
                                  'document_version': version, 'field': 'text',
                                  'text': selected, 'start': start, 'end': start + len(selected)})
    return value


def prepare(binary):
    assert not (RUN / 'freeze.json').exists()
    documents = read(RUN / 'primary-extract/tool-result.json')['structuredContent']['documents']
    assert len(documents) == 2 and all(d['status'] == 'ok' for d in documents)
    prepared = RUN / 'prepared'
    prepared.mkdir()
    pairs = [('P01', 'regression', 'T01'), ('P02', 'candidate', 'N04')]
    for index, (name, group, case) in enumerate(pairs):
        capture = ROOT / 'runs/search-repair-2026-11-18' / group / 'research' / case / 'capture/0000-research'
        baseline = restore(capture)
        document = documents[index]
        qinput = prepared / (name + '-qwen.json')
        save(qinput, {'question': baseline['context']['question'], 'extracted': document})
        output = prepared / (name + '-qwen')
        env = dict(os.environ, PRIMARY_QWEN_INPUT=str(qinput.resolve()), PRIMARY_QWEN_OUTPUT=str(output.resolve()))
        with (prepared / (name + '-qwen.log')).open('x') as log:
            code = subprocess.run([str(binary.resolve()), QWEN_TEST, '--exact', '--ignored', '--nocapture'],
                                  env=env, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=45).returncode
        assert code == 0, 'Local Qwen failed; no retry/fallback'
        selection = read(output / 'selected.json')
        candidate = add_primary(baseline, document, selection['selected_ranges'])
        common_budget = None
        for variant, fixture in [('baseline', baseline), ('primary', candidate)]:
            path = prepared / (name + '-' + variant + '.json')
            save(path, fixture)
            dest = prepared / (name + '-' + variant + '-prepare')
            assert shared.invoke(binary, path, dest, 'prepare') == 0
            context = json.loads(read(dest / 'prepared-context.json')['context_text'])
            if variant == 'baseline':
                common_budget = context['budget']
                baseline_context = context
            else:
                context['budget'] = common_budget
                for field in baseline_context:
                    if field not in ['sources', 'rejection_context']:
                        assert context[field] == baseline_context[field], 'Unexpected changed material: ' + field
                assert context['sources'][:len(baseline_context['sources'])] == baseline_context['sources']
                assert context['rejection_context'][:len(baseline_context['rejection_context'])] == baseline_context['rejection_context']
            fixture['context_text'] = json.dumps(context, ensure_ascii=False, separators=(',', ':'))
            assert len(fixture['context_text']) <= 24000
            path.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + '\n')
    (RUN / 'bin').mkdir()
    shutil.copy2(binary, RUN / 'bin/audit-tests')
    files = [ROOT / 'scripts/primary-evidence-2026-11-18.py', ROOT / 'scripts/primary-evidence-fixture.py',
             ROOT / 'scripts/scoped-audit-2026-11-18.py', ROOT / 'src/research_scope_audit_experiment.rs',
             ROOT / 'src/research_tests.rs', ROOT / 'src/research.rs',
             ROOT / 'feasibility/primary-evidence-cases-2026-11-18.json', RUN / 'case-index.json',
             RUN / 'gold-review.json', RUN / 'primary-extract/tool-result.json', RUN / 'bin/audit-tests',
             *sorted(prepared.glob('*.json')), *sorted(prepared.glob('*/selected.json'))]
    order = []
    for replicate in range(1, 4):
        for index, name in enumerate(['P01', 'P02']):
            variants = ['baseline', 'primary'] if (replicate + index) % 2 else ['primary', 'baseline']
            order.extend({'case': name, 'replicate': replicate, 'variant': v} for v in variants)
    save(RUN / 'freeze.json', {'files': {str(p.relative_to(ROOT)): sha(p) for p in files}, 'order': order,
         'model': 'deepseek/deepseek-v4.1-flash-1017', 'max_calls': 12, 'wall_seconds': 600,
         'policy': 'Same original claim/ownquote and generic audit/schema; primary source only rejection context. Three preplanned paired replicates, not conditional retries. First error stops.'})
    print('Frozen 2 batches, 2 variants, 3 replicates; no generative calls yet')


def run():
    freeze = read(RUN / 'freeze.json')
    for path, digest in freeze['files'].items():
        assert sha(ROOT / path) == digest, path
    (RUN / 'attempts').mkdir()
    started = time.monotonic()
    attempts = []
    stop = None
    for item in freeze['order']:
        if len(attempts) >= 12 or time.monotonic() - started >= 492:
            stop = {'reason': 'campaign_budget'}
            break
        time.sleep(3)
        output = RUN / 'attempts' / ('r' + str(item['replicate']) + '-' + item['case'] + '-' + item['variant'])
        begin = time.monotonic()
        try:
            code = shared.invoke(RUN / 'bin/audit-tests', RUN / 'prepared' / (item['case'] + '-' + item['variant'] + '.json'), output, 'baseline')
            row = dict(item, exit_code=code, elapsed_ms=round((time.monotonic() - begin) * 1000))
            if code != 0 or not (output / 'result.json').exists():
                row['error'] = read(output / 'error.json') if (output / 'error.json').exists() else {'status': 'invalid_or_failed_audit'}
        except subprocess.TimeoutExpired:
            row = dict(item, error={'status': 'runner_timeout'})
        attempts.append(row)
        print(json.dumps(row), flush=True)
        if row.get('error'):
            stop = {'reason': 'first_failed_attempt_no_retry', 'attempt': row}
            break
    if stop:
        save(RUN / 'stop.json', stop)
    save(RUN / 'summary.json', {'planned': 12, 'attempts': attempts, 'stop': stop, 'retries': 0})


def self_check():
    base = {'context': {'sources': [], 'chunks': [], 'claims': [{'claim': {'text': 'fixed'}}]}}
    text = '😀 Known original source statement.'
    document = {'text': text, 'text_hash': hashlib.sha256(text.encode()).hexdigest(), 'final_url': 'https://example.org/reference'}
    result = add_primary(base, document, [(5, len(text.encode()))])
    assert result['context']['chunks'][0]['start'] == 2
    assert result['context']['chunks'][0]['text'] == text[2:]
    assert result['context']['claims'] == base['context']['claims']
    assert not base['context']['sources']
    try:
        add_primary(base, dict(document, text_hash='wrong'), [(0, 20)])
    except AssertionError:
        print('PASS: original UTF8-to-Unicode ranges, unchanged claims and sourcehash guard')
    else:
        raise AssertionError('Bad source hash accepted')


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
