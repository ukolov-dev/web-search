"""Blind source annotation: reads source snapshots only, never predicted labels."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/reliability-holdout-2026-10-30')
SPECS = {
    'h01': {
        'C002': ['there is no guarantee that the action will be carried out', 'processing has not been completed'],
        'C005': ['processing has not been completed or may not have started', 'processing of the request is not guaranteed'],
        'C014': ['요청이 접수되었지만 아직 처리가 완료되지 않았을 때 반환됩니다.'],
    },
    'h02': {
        'C018': ['시퀀스는 RollBack이 되지 않습니다.', '서로 다른 세션에서 nextval 시퀀스 값을 조회하면 cache_size 만큼 차이가 날 수'],
        'C019': ['This causes gaps in numbering from aborted transactions', 'Another cause for gaps in sequence is the use of the CACHE clause'],
    },
    'h03': {
        'C011': ['the resulting iterator will raise a ValueError if the arguments are exhausted at differing lengths', 'This error will occur at the point when iteration would normally stop today.'],
        'C015': ['if you set strict to True', 'list(zip(range(5), range(100), strict=True))', 'ValueError: zip() argument 2 is longer than argument 1'],
        'C017': ['except ValueError', '실제로 원소를 꺼내는 과정에서 불일치를 발견해요.'],
    },
    'h04': {'C005': ['replacing the original file if to already exists', 'from and to are on separate filesystems']},
    'h05': {
        'C006': ['on-failure', "The on-failure policy only prompts a restart if the container exits with a failure. It doesn't restart the container if the daemon restarts.", 'unless-stopped'],
        'C008': ['It does not restart the container simply because the Docker daemon restarts.', 'unless-stopped'],
        'C015': ['on-failure[:max-retries] Restarts only if the container exits with a non-zero exit code.', 'unless-stopped Same as always but does not restart if the container was manually stopped before the daemon restarted.'],
        'C019': ['on-failure does not survive one', 'unless-stopped behaves like always'],
    },
    'h06': {'C014': ['extra CPU, memory, disk space, and disk I/O overhead', 'prevent the reuse of ROWIDs from previously deleted rows']},
    'h07': {'C011': ['--dry-run Don’t actually remove anything', '-X Remove only files ignored by Git.', '-x Don’t use the standard ignore rules']},
    'h08': {'C005': ['display 속성을 none으로 설정하세요.', '접근성 트리에서 제외됩니다.']},
}
NEGATIVE = {'h04': ['C001', 'C018', 'C020']}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


class Text(HTMLParser):
    def __init__(self):
        super().__init__()
        self.parts = []

    def handle_data(self, data):
        self.parts.append(data)


def build():
    dest = Path('feasibility/reliability-holdout-gold-v1.json')
    assert not dest.exists(), 'gold already frozen; never silently relabel after unsealing'
    rows = []
    for row in load(ROOT / 'retrieval/corpus.json')['cases']:
        cid = row['case']['id']
        gold = []
        for candidate in row['candidates']:
            ident = candidate['id']
            cache = ROOT / 'page-cache' / (sha(candidate['url'].encode()) + '.json')
            doc = load(cache).get('document') if cache.exists() else None
            path = cache
            audit = ROOT / 'gold-audit' / f'{cid}-{ident}-document.json'
            if doc is None and audit.exists():
                doc, path = load(audit), audit
            entry = dict(id=ident, url=candidate['url'], grade=None,
                         basis='unjudged: no sufficient source snapshot', support=[])
            if doc:
                assert sha(doc['text'].encode()) == doc['text_hash']
                text = ' '.join(doc['text'].split())
                entry.update(snapshot=str(path), text_hash=doc['text_hash'], snapshot_chars=len(text))
                if 500 <= len(text) < 100000:
                    entry.update(grade=1, basis='captured source helps topic but lacks the requested relationship or a material queried condition')
                if cid == 'h07' and ident == 'C011':
                    # Source gold may inspect retained HTML; the missing flags are NOT injected into either branch.
                    path = ROOT / 'fixed/h07-C011.html'
                    parser = Text()
                    parser.feed(path.read_text())
                    text = ' '.join(' '.join(parser.parts).split())
                    entry.update(snapshot=str(path), snapshot_sha256=sha(path.read_bytes()),
                                 extraction_gap='captured extracted text contains only Interactive mode; flags exist in original HTML')
                for needle in SPECS.get(cid, {}).get(ident, []):
                    match = re.search(re.escape(needle), text, re.I)
                    assert match, (cid, ident, needle)
                    start, end = match.span()
                    entry['support'].append(dict(quote=text[start:end], start=len(text[:start].encode()), end=len(text[:end].encode())))
                if entry['support']:
                    entry.update(grade=2, basis='source provides concrete evidence for requested relation, including material conditions; negative answer to premise counts')
            if ident in NEGATIVE.get(cid, []):
                entry.update(grade=0, basis='different API/product/navigation; no requested std::fs::rename relationship')
            gold.append(entry)
        rows.append(dict(id=cid, query=row['case']['query'], gold=gold))
    result = dict(protocol=dict(author='single author blinded to both branches predicted labels; not independent acceptance',
                               freeze='queries/code frozen before inference; gold frozen after inference but before inspecting predictions',
                               scope='known relevant retrieved sources; NOT universal web recall; null remains unjudged',
                               criteria='grade2 concrete requested relationship with material conditions; grade1 partial topic; grade0 unrelated; short/failed snapshots unknown unless positive witness',
                               source_gold='union of retained extracted snapshots plus two gold-only primary fetches; h07 official retained HTML manually inspected because extraction lost flags',
                               isolation='gold and gold-only documents never passed to branches; normal page selection and budgets retained'), cases=rows)
    dest.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    inputs = [dest, Path('feasibility/reliability-holdout-cases-v1.json'), ROOT / 'retrieval/corpus.json']
    (ROOT / 'gold-freeze.json').write_text(json.dumps({str(p): sha(p.read_bytes()) for p in inputs}, indent=2) + '\n')
    print('known', sum(g['grade'] is not None for r in rows for g in r['gold']),
          'positive', sum(g['grade'] == 2 for r in rows for g in r['gold']))


if __name__ == '__main__':
    build()
