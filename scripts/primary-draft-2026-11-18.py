#!/usr/bin/env python3
"""Frozen oracle-plan/source NEWDRAFT→unchanged audit experiment; no retrieval verdict."""
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
RUN = ROOT / 'runs/primary-draft-2026-11-18'
TEST = 'research::tests::section_pipeline_experiment::primary_saved_draft_audit'
ORDER = [('D01', 'primary'), ('D02', 'primary'), ('D01', 'baseline'), ('D02', 'baseline')]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write('\n')


def fixture(case, arm, facets, metadata):
    source = case['sources'][arm]
    text = source['source_text']
    assert 0 < len(text) <= 100000
    digest = hashlib.sha256(text.encode()).hexdigest()
    assert digest == source['source_text_sha256']
    assert 1 <= len(facets) <= 4 and all(isinstance(f, str) and 0 < len(f) <= 160 for f in facets)
    # Explicit allowlist: neither old claims nor gold can enter a new generation.
    return {'question': case['question'], 'language': case['language'], 'facets': facets,
            'title': source['title'], 'url': source['url'], 'metadata': metadata,
            'extracted': {'text': text, 'text_hash': digest,
                          'extraction_method': source['extraction']['method'],
                          'content_scope': source['extraction']['scope'],
                          'truncated': source['extraction']['truncated']}}


def prepare(binary):
    assert not (RUN / 'freeze.json').exists(), 'Already frozen; no overwrite'
    index_path = RUN / 'case-index.json'
    index = read(index_path)
    cases = {case['id']: case for case in index['cases']}
    assert set(cases) == {'D01', 'D02'} and len(index['cases']) == 2
    prepared = RUN / 'prepared'
    prepared.mkdir()
    inputs = [index_path, ROOT / "feasibility/primary-draft-cases-2026-11-18.json", RUN / "gold-review.json"]
    for number, case_id in enumerate(['D01', 'D02'], 1):
        case = cases[case_id]
        original = ROOT / f'runs/primary-evidence-2026-11-18/prepared/P0{number}-baseline.json'
        context = read(original)['context']
        assert context['question'] == case['question'] and context['language'] == case['language']
        facets = context['facets']
        inputs.append(original)
        for arm in ['primary', 'baseline']:
            source = case['sources'][arm]
            artifact = ROOT / source['source_artifact']
            assert sha(artifact) == source['source_artifact_sha256']
            inputs.append(artifact)
            if arm == 'baseline':
                saved = read(artifact)
                assert saved['text'] == source['source_text'] and saved['url'] == source['url']
                metadata = saved['metadata']
            else:
                documents = read(artifact)['structuredContent']['documents']
                matches = [d for d in documents if d['final_url'] == source['url']]
                assert len(matches) == 1 and matches[0]['status'] == 'ok'
                document = matches[0]
                assert document['text'] == source['source_text']
                # Native document metadata copied by case-index, not compact audit publication.
                metadata = document['news']
                assert isinstance(metadata, dict) and metadata == source['publication']
            output = prepared / f'{case_id}-{arm}.json'
            save(output, fixture(case, arm, facets, metadata))
            inputs.append(output)
    (RUN / 'bin').mkdir()
    stable = RUN / 'bin/primary-draft-tests'
    shutil.copy2(binary.resolve(), stable)
    listing = subprocess.run([str(stable), '--list'], capture_output=True, text=True, check=True)
    assert TEST + ': test' in listing.stdout, 'Exact ignored test absent from binary'
    save(RUN / 'binary-list.json', {'stdout': listing.stdout})
    code = [ROOT / 'scripts/primary-draft-2026-11-18.py', ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
            *sorted((ROOT / 'src').glob('*.rs')), stable, RUN / 'binary-list.json']
    save(RUN / 'freeze.json', {'files': {str(p.relative_to(ROOT)): sha(p) for p in dict.fromkeys(inputs + code)},
         'order': [{'case': c, 'arm': a} for c, a in ORDER], 'test': TEST,
         'qwen_settings_sha256': sha(Path.home() / '.omlx/settings.json'),
         'model': 'deepseek/deepseek-v4.1-flash-1017', 'max_trials': 4, 'max_generations': 24,
         'per_trial_generations': 6, 'per_trial_session_seconds': 180,
         'process_timeout_seconds': 200, 'pace_seconds': 10,
         'scope': 'Frozen original model facets; known saved full source; new draft and unchanged old audit. No gold or old claims as model input. Not a retrieval test.'})
    print('Prepared and frozen four trials; zero generative/source calls')


def failure(result, output):
    if result.get('error'):
        return 'result_error'
    if result.get('stop_reason') in ['model_error', 'deadline', 'sources_unavailable']:
        return 'failed_stop_reason'
    if list(output.glob('request-rejected-*.json')):
        return 'model_rejected'
    for action in result.get('actions', []):
        if action.get('action') == 'qwen' and action.get('status') != 'ok':
            return 'qwen_failure'
    if any(call.get('status') != 'returned' for call in result.get('model_calls', [])):
        return 'model_call_failure'
    return None


def run():
    frozen = read(RUN / 'freeze.json')
    assert frozen['order'] == [{'case': c, 'arm': a} for c, a in ORDER]
    for path, digest in frozen['files'].items():
        assert sha(ROOT / path) == digest, 'Freeze mismatch: ' + path
    assert sha(Path.home() / '.omlx/settings.json') == frozen['qwen_settings_sha256']
    attempts_dir = RUN / 'attempts'
    attempts_dir.mkdir()  # A prior attempt, including crash/timeout, cannot be resumed.
    attempts = []
    generations = 0
    stop = None
    for index, item in enumerate(frozen['order']):
        if index:
            time.sleep(10)
        name = item['case'] + '-' + item['arm']
        output = attempts_dir / name
        env = dict(os.environ, SECTION_PIPELINE_INPUT=str((RUN / 'prepared' / (name + '.json')).resolve()),
                   SECTION_PIPELINE_OUTPUT=str(output.resolve()), SECTION_PIPELINE_PACE_SECONDS='10')
        row = dict(item, status='attempted')
        started = time.monotonic()
        try:
            with (attempts_dir / (name + '.log')).open('x') as log:
                process = subprocess.run([str(RUN / 'bin/primary-draft-tests'), TEST, '--exact', '--ignored', '--nocapture'],
                                         cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=200)
            row['exit_code'] = process.returncode
            if process.returncode != 0:
                row['error'] = 'nonzero_exit'
            if (output / 'result.json').exists():
                result = read(output / 'result.json')
                count = result['budget']['used']['generations']
                assert type(count) is int and 0 <= count <= 6
                generations += count
                row['generations'] = count
                reason = failure(result, output)
                if reason:
                    row['error'] = reason
            else:
                row['error'] = 'missing_result'
            assert generations <= 24
        except subprocess.TimeoutExpired:
            row['error'] = 'process_timeout'
        except Exception as error:
            row['error'] = 'runner_exception:' + type(error).__name__
        row['elapsed_ms'] = round((time.monotonic() - started) * 1000)
        attempts.append(row)
        save(attempts_dir / (name + '-attempt.json'), row)
        print(json.dumps(row), flush=True)
        if row.get('error'):
            stop = 'first_error_no_retry'
            break
    save(RUN / 'summary.json', {'planned': 4, 'attempts': attempts,
         'not_run': frozen['order'][len(attempts):], 'stop': stop, 'retries': 0,
         'recorded_generations': generations, 'unrecorded_usage_possible': any('generations' not in a for a in attempts)})


def self_check():
    text = '😀 A complete original condition.'
    digest = hashlib.sha256(text.encode()).hexdigest()
    source = {'source_text': text, 'source_text_sha256': digest, 'title': 'Source',
              'url': 'https://example.org/', 'extraction': {'method': 'main', 'scope': 'main', 'truncated': False}}
    case = {'question': 'Question?', 'language': 'en', 'sources': {'primary': source},
            'gold_aspects': ['FORBIDDEN'], 'claims': ['OLD']}
    result = fixture(case, 'primary', ['Original model facet'], {'title': 'Native'})
    assert result['extracted']['text'] == text and result['metadata'] == {'title': 'Native'}
    assert 'FORBIDDEN' not in json.dumps(result) and 'OLD' not in json.dumps(result)
    source['source_text_sha256'] = 'wrong'
    try:
        fixture(case, 'primary', ['Facet'], {})
    except AssertionError:
        pass
    else:
        raise AssertionError('Bad hash accepted')
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)
        assert failure({'actions': [{'action': 'qwen', 'status': 'failed_lexical_fallback'}]}, path) == 'qwen_failure'
        assert failure({'error': 'HTTP 429'}, path) == 'result_error'
        assert failure({'model_calls': [{'status': 'error'}]}, path) == 'model_call_failure'
        save(path / 'request-rejected-01.json', {})
        assert failure({}, path) == 'model_rejected'
        try:
            save(path / 'request-rejected-01.json', {})
        except FileExistsError:
            pass
        else:
            raise AssertionError('Overwrite allowed')
    assert len(ORDER) == 4 and ORDER[:2] == [('D01', 'primary'), ('D02', 'primary')]
    print('PASS: allowlisted newdraft fixtures, Unicode/hash, stop guards, no overwrite, fixed order')


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
