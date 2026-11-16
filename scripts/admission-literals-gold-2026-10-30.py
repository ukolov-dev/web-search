"""Source-only NEW24 gold; no model, network, branch extraction or prediction reads."""
import argparse
import datetime
import hashlib
import json
import re
import runpy
from pathlib import Path

ROOT = Path('runs/admission-literals-2026-10-30')
CASES = Path('feasibility/admission-literals-cases-v1.json')
PLAN = Path('feasibility/admission-literals-plan-v1.json')
DEST = Path('feasibility/admission-literals-gold-v1.json')
AUDIT = ROOT / 'gold-audit'
GROUPS = ('l01-l08', 'l09-l16', 'l17-l24')
CANONICAL = Path(__file__).with_name('passage-coverage-confirm-gold-2026-10-30.py')
source_text = runpy.run_path(str(CANONICAL))['source_text']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def write_new(path, value):
    body = json.dumps(value, ensure_ascii=False, indent=2) + '\n'
    if path.exists():
        assert path.read_text() == body, ('immutable artifact changed', str(path))
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)


def before_inference():
    assert not DEST.exists(), 'gold already frozen; do not relabel after inference'
    for arm in ('baseline', 'fixed'):
        assert not (ROOT / arm / 'results.json').exists(), 'gold must freeze before either arm inference'
        assert not list((ROOT / arm).glob('*-model-*.json')), 'model artifacts already present'


def inputs():
    before_inference()
    assert (ROOT / 'code-freeze.json').exists(), 'freeze both code variants before source annotation'
    for path, digest in load(ROOT / 'code-freeze.json').items():
        assert sha(Path(path).read_bytes()) == digest, ('changed frozen code/input', path)
    cases, plan, pool = load(CASES), load(PLAN), load(ROOT / 'retrieval/corpus.json')
    assert plan['cases_sha256'] == sha(CASES.read_bytes())
    assert len(cases['cases']) == 24
    assert [r['case'] for r in pool['cases']] == cases['cases'], 'retrieval query/forms changed'
    assert (AUDIT / 'cache-complete-confirmation.json').exists(), 'raw cache must be complete before annotation'
    criteria = {r['id']: r for r in plan['case_criteria']}
    assert set(criteria) == {c['id'] for c in cases['cases']}
    return cases, plan, pool, criteria


def sources(pool):
    acquisition = load(ROOT / 'preacquisition.json')
    assert acquisition['complete'] is True, 'finish all raw pre-acquisition before annotation'
    acquisition_errors = {d['url']: d.get('error') for d in acquisition['documents']}
    out = {}
    for row in pool['cases']:
        cid = row['case']['id']
        for candidate in row['candidates']:
            ident, url = candidate['id'], candidate['url']
            path = ROOT / 'raw-cache' / (sha(url.encode()) + '.json')
            saved = load(path)  # Missing URL is a hard error, never a network fallback.
            assert saved['schema'] == 'raw-document-v1' and saved['url'] == url
            assert url in acquisition_errors, 'candidate absent from pre-acquisition manifest'
            entry = dict(case=cid, id=ident, url=url, snapshot=str(path), raw_snapshot_sha256=sha(path.read_bytes()),
                         acquisition_error=acquisition_errors[url])
            if saved.get('fetched') is None:
                assert saved.get('error') and saved.get('html_hash') is None
                entry.update(available=False, error=saved['error'], text=None)
            else:
                assert saved.get('error') is None
                html = saved['fetched']['html']
                assert sha(html.encode()) == saved['html_hash']
                text = source_text(html)
                entry.update(available=True, html_hash=saved['html_hash'], normalized_text_hash=sha(text.encode()),
                             source_chars=len(text), final_url=saved['fetched']['url'], http_status=saved['fetched']['status'], text=text,
                             source_text_path=str(AUDIT / 'source-text' / f'{cid}-{ident}.txt'))
            assert (cid, ident) not in out
            out[cid, ident] = entry
    return out


def prepare():
    _, _, pool, criteria = inputs()
    snapshots = sources(pool)
    for entry in snapshots.values():
        if entry['available']:
            path = Path(entry['source_text_path'])
            path.parent.mkdir(parents=True, exist_ok=True)
            body = entry['text'] + '\n'
            if path.exists():
                assert path.read_text() == body
            else:
                path.write_text(body)
    inventory = dict(protocol='All retrieved candidate raw sources, complete normalized original content; no branch inputs or predictions.',
                     criteria=criteria, sources=[{k: v for k, v in e.items() if k != 'text'} for e in snapshots.values()])
    write_new(AUDIT / 'source-inventory.json', inventory)
    print(json.dumps(dict(candidates=len(snapshots), available=sum(e['available'] for e in snapshots.values()),
                          unavailable=sum(not e['available'] for e in snapshots.values())), indent=2))


def proof(text, needle):
    assert isinstance(needle, str) and len(needle) >= 12, 'quote must be a concrete source excerpt'
    match = re.search(re.escape(needle), text, re.I)
    assert match, ('quote absent from complete canonical source', needle)
    start, end = match.span()
    result = dict(quote=text[start:end], start=len(text[:start].encode()), end=len(text[:end].encode()))
    assert text.encode()[result['start']:result['end']].decode() == result['quote']
    return result


def build(freeze):
    _, _, pool, criteria = inputs()
    snapshots = sources(pool)
    annotations, shard_paths = {}, []
    for group in GROUPS:
        path = AUDIT / f'annotations-{group}.json'
        shard_paths.append(path)
        shard = load(path)
        assert shard['blinded_to_predictions'] is True
        for row in shard['cases']:
            assert row['id'] in criteria
            for label in row['gold']:
                key = row['id'], label['id']
                assert key in snapshots and key not in annotations, ('unknown or repeated source', key)
                annotations[key] = label
    assert set(annotations) == set(snapshots), 'every retrieved candidate needs inspected grade or explicit unknown'
    rows = []
    inspected = {}
    for row in pool['cases']:
        cid = row['case']['id']
        gold = []
        for c in row['candidates']:
            key = cid, c['id']
            source, label = snapshots[key], annotations[key]
            grade = label['grade']
            assert grade in (None, 0, 1, 2)
            assert isinstance(label['basis'], str) and label['basis'].strip()
            entry = {k: v for k, v in source.items() if k not in ('case', 'text', 'available', 'source_text_path')}
            entry.update(grade=grade, basis=label['basis'], support=[])
            if not source['available'] or source.get('acquisition_error') or source.get('http_status') != 200:
                assert grade is None, ('unobserved/challenge/non200 source cannot be labeled', key)
            elif grade is not None:
                assert label.get('full_source_review') is True and label.get('searched_terms'), ('complete source scan required', key)
                entry['field'] = 'raw_html_text'
                entry['inspection'] = dict(full_source_review=True, searched_terms=label['searched_terms'], checked_late_sections=True)
                for field in ('support', 'partial_support', 'negative_support'):
                    if label.get(field):
                        entry[field] = [proof(source['text'], q) for q in label[field]]
                if grade == 2:
                    assert entry['support'], ('complete source witnesses required', key)
                elif grade == 1:
                    assert entry.get('partial_support'), ('concrete partial witnesses required', key)
                else:
                    assert entry.get('negative_support'), ('source-bound unrelated/contrary scope proof required', key)
                inspected[key] = grade
            if label.get('scope_warning'):
                entry['scope_warning'] = label['scope_warning']
            gold.append(entry)
        rows.append(dict(id=cid, query=row['case']['query'], criteria=' '.join(criteria[cid]['material_criteria']), gold=gold))
    peers, peer_paths = {}, []
    if freeze:
        for group in GROUPS:
            path = AUDIT / f'peer-{group}.json'
            peer_paths.append(path)
            peer = load(path)
            assert peer['blinded_to_predictions'] is True
            for item in peer['reviewed']:
                key = item['case'], item['id']
                assert key not in peers and key in inspected
                assert item['accepted'] is True and item['grade'] == inspected[key] and item['basis'].strip()
                peers[key] = item
        assert set(peers) == set(inspected), 'rotated peer must check every inspected source grade'
    result = dict(protocol=dict(author='three source-only annotators, blinded to branch outputs and model calls',
                               freeze='source gold and rotated peer review frozen before either model arm inference',
                               scope='retrieved candidate pool only; null is unknown, not a negative',
                               source_gold='complete immutable original raw HTML, canonical visible text and publisher-declared typed code-view rawLines',
                               offsets='UTF-8 byte offsets in whitespace-normalized source_text(original HTML)',
                               independent_acceptance=False,
                               artifact_status='frozen before inference' if freeze else 'source-only review draft'), cases=rows)
    dest = DEST if freeze else AUDIT / 'annotation-draft.json'
    write_new(dest, result)
    if freeze:
        paths = [dest, CASES, PLAN, ROOT / 'retrieval/corpus.json', ROOT / 'preacquisition.json', ROOT / 'code-freeze.json', AUDIT / 'cache-complete-confirmation.json',
                 AUDIT / 'source-inventory.json', Path(__file__), CANONICAL, *shard_paths, *peer_paths]
        paths.extend(Path(e['snapshot']) for e in snapshots.values())
        paths = sorted(set(paths))
        write_new(ROOT / 'gold-freeze.json', {str(p): sha(p.read_bytes()) for p in paths})
        write_new(AUDIT / 'gold-before-inference.json', dict(frozen_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  gold_sha256=sha(DEST.read_bytes()), baseline_results_present=False, fixed_results_present=False,
                  inspected_sources=len(inspected), peer_reviewed_sources=len(peers), candidate_sources=len(snapshots)))
    print(json.dumps(dict(candidates=len(snapshots), inspected=len(inspected), positive=sum(v == 2 for v in inspected.values()),
                          partial=sum(v == 1 for v in inspected.values()), unrelated=sum(v == 0 for v in inspected.values()),
                          unknown=len(snapshots)-len(inspected), frozen=freeze), indent=2))


def self_check():
    text = source_text('<p>Шаг 1: <code>Value</code>.</p><script>untrusted()</script><p>Next</p>')
    assert text == 'Шаг 1: Value . Next'
    q = proof(text, 'Шаг 1: Value .')
    assert q['start'] == 0 and q['end'] == len('Шаг 1: Value .'.encode())
    try:
        proof(text, 'absent exact quote')
    except AssertionError:
        pass
    else:
        raise AssertionError('absent source quote accepted')
    print('canonical source / exact UTF-8 / absent quote self-check passed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('prepare', 'draft', 'freeze', 'self-check'))
    mode = parser.parse_args().mode
    if mode == 'self-check':
        self_check()
    elif mode == 'prepare':
        prepare()
    else:
        build(freeze=mode == 'freeze')
