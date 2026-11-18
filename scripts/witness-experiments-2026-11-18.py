#!/usr/bin/env python3
"""Offline fixed-witness comparison; gold is never supplied to candidate functions."""
import argparse
import hashlib
import importlib.util
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/witness-experiments-2026-11-18'
CASES = ROOT / 'feasibility/witness-experiments-cases-2026-11-18.json'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path):
    return json.loads(path.read_text())


def check_slice(text, witness):
    a, b = witness['start'], witness['end']
    assert type(a) is int and type(b) is int and 0 <= a < b <= len(text)
    assert text[a:b] == witness['quote'], 'Frozen witness mismatch'


def check_output(text, witness, value, arm):
    if value.get('quote') is None:
        assert value['status'] in ['withhold', 'withheld']
        assert value.get('start') is None and value.get('end') is None
        assert value.get('claim') is None
        return False
    check_slice(text, value)
    assert value['start'] <= witness['start'] < witness['end'] <= value['end']
    assert 12 <= len(value['quote']) <= 360
    assert value['status'] in ['expanded', 'unchanged', 'ok']
    if arm == 'B':
        assert value['claim'] == ' '.join(value['quote'].split())
        assert 1 <= len(value['claim']) <= 240
    return True


def self_check():
    text = '😀 Intro. Returns bytes only if enabled.'
    start = text.index('Returns')
    witness = {'start': start, 'end': start + 7, 'quote': 'Returns'}
    result = {'status': 'ok', 'start': start, 'end': len(text),
              'quote': text[start:], 'claim': text[start:]}
    assert check_output(text, witness, result, 'B')
    for changed in [dict(result, quote='invented text'),
                    dict(result, claim='Returns bytes always.'),
                    dict(result, start=start + 1), dict(result, status='withheld')]:
        try:
            check_output(text, witness, changed, 'B')
        except AssertionError:
            pass
        else:
            raise AssertionError('Bad candidate accepted')
    assert not check_output(text, witness, {'status': 'withhold', 'reason': 'ambiguous'}, 'A')
    print('PASS: exact Unicode ranges, containment, whitespace-only claim and explicit withholding')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    if args.self_check:
        return self_check()
    freeze = load(RUN / 'freeze.json')
    for path, digest in freeze['files'].items():
        assert sha(ROOT / path) == digest, 'Freeze mismatch: ' + path
    metadata = {r['id']: r for r in load(CASES)['cases']}
    inputs = load(RUN / 'inputs.json')['cases']
    assert len(metadata) == len(inputs) == len({r['id'] for r in inputs})
    helpers = {}
    for arm, path in freeze['helpers'].items():
        spec = importlib.util.spec_from_file_location('candidate_' + arm, ROOT / path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        helpers[arm] = module.expand if arm == 'A' else module.extract
    rows = []
    for case in inputs:
        gold = metadata[case['id']]
        text, witness = case['source_text'], case['witness']
        assert len(text) <= 100_000
        assert hashlib.sha256(text.encode()).hexdigest() == gold['source_text_sha256']
        assert witness == gold['witness'] and case['claim_text'] == gold['claim_text']
        check_slice(text, witness)
        row = {'id': case['id'], 'cohort': case['cohort'],
               'baseline': {'claim': case['claim_text'], 'evidence': witness}, 'arms': {}}
        for arm, helper in helpers.items():
            value = helper(text, witness['start'], witness['end'])
            emitted = check_output(text, witness, value, arm)
            row['arms'][arm] = {'emitted': emitted, 'candidate': value,
                               'claim': (case['claim_text'] if arm == 'A' else value['claim']) if emitted else None}
        rows.append(row)
    result = {'schema': 'offline-witness-results-v1', 'freeze': freeze, 'rows': rows,
              'network_calls': 0, 'model_calls': 0, 'semantic_scoring': 'separate independent review'}
    with (RUN / 'results.json').open('x') as target:
        json.dump(result, target, ensure_ascii=False, indent=2)
        target.write('\n')
    print(json.dumps({'rows': len(rows), 'emitted': {a: sum(r['arms'][a]['emitted'] for r in rows) for a in helpers}}))


if __name__ == '__main__':
    main()
