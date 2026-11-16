"""Offline extract-file comparison on the same captured HTML, with no fetch or LLM."""
import collections
import hashlib
import json
import re
import runpy
import subprocess
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/extraction-selection-2026-10-30')
OUT = ROOT / 'extraction-comparison'
DEST = Path('feasibility/extraction-selection-extraction-2026-10-30.json')
BINARIES = {'baseline': Path('target/debug/web-search-baseline-extraction'),
            'fixed': Path('target/debug/web-search')}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(Path(path).read_text())


def dump(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


class Marker(HTMLParser):
    def __init__(self):
        super().__init__()
        self.bodies = 0

    def handle_starttag(self, tag, attrs):
        self.bodies += any(name == 'data-pagefind-body' for name, _ in attrs)


def compare(ident, url, html, source_text):
    fixture = OUT / (ident + '.html')
    fixture.write_text(html)
    assert sha(fixture.read_bytes()) == sha(html.encode())
    marker = Marker()
    marker.feed(html)
    row = dict(id=ident, url=url, html_hash=sha(html.encode()), html_bytes=len(html.encode()),
               source_chars=len(source_text(html)), pagefind_body_count=marker.bodies, branches={})
    for branch, binary in BINARIES.items():
        path = OUT / f'{ident}-{branch}.json'
        command = [str(binary.resolve()), 'extract-file', str(fixture.resolve()), url,
                   '--output', str(path.resolve())]
        run = subprocess.run(command, text=True, capture_output=True, timeout=30, check=False)
        dump(OUT / f'{ident}-{branch}-process.json',
             dict(command=command, exit_code=run.returncode, stdout=run.stdout, stderr=run.stderr))
        result = dict(exit_code=run.returncode, output=str(path))
        if run.returncode == 0:
            doc = load(path)
            assert sha(doc['text'].encode()) == doc['text_hash']
            text = ' '.join(doc['text'].split())
            result.update(text_hash=doc['text_hash'], normalized_text_hash=sha(text.encode()),
                          chars=doc['chars'], normalized_chars=len(text),
                          extraction_method=doc['extraction_method'], content_scope=doc['content_scope'],
                          truncated=doc['truncated'], warnings=doc['warnings'],
                          flags={flag: bool(re.search(r'(?<![\w-])' + re.escape(flag) + r'(?![\w-])', text))
                                 for flag in ['-n', '--dry-run', '-x', '-X']})
        else:
            result['error'] = run.stderr.strip()
        row['branches'][branch] = result
    a, b = row['branches']['baseline'], row['branches']['fixed']
    row['same_extracted_text'] = a.get('text_hash') == b.get('text_hash') if a['exit_code'] == b['exit_code'] == 0 else None
    return row


def add_gold_quote_coverage(rows):
    gold = load('feasibility/extraction-selection-gold-v1.json')
    for row in rows:
        quotes = {support['quote'] for case in gold['cases'] for g in case['gold']
                  if g['url'] == row['requested_url'] and g['grade'] == 2 for support in g['support']}
        row['gold_support_quotes'] = len(quotes)
        for result in row['branches'].values():
            if result['exit_code'] == 0:
                text = ' '.join(load(result['output'])['text'].split())
                result['gold_support_quotes_found'] = sum(quote in text for quote in quotes)


def main():
    assert not OUT.exists() and not DEST.exists(), 'preserve existing extraction comparison'
    for path, digest in load(ROOT / 'code-freeze.json').items():
        assert sha(Path(path).read_bytes()) == digest, ('runtime changed after freeze', path)
    binaries = {branch: dict(path=str(path), sha256=sha(path.read_bytes())) for branch, path in BINARIES.items()}
    assert binaries['baseline']['sha256'] != binaries['fixed']['sha256']
    OUT.mkdir()
    source_text = runpy.run_path('scripts/extraction-selection-gold-2026-10-30.py')['source_text']
    rows, failed_acquisitions = [], []
    for path in sorted((ROOT / 'raw-cache').glob('*.json')):
        if path.name == 'blocks.json':
            continue
        snapshot = load(path)
        if not snapshot.get('fetched'):
            failed_acquisitions.append(dict(snapshot=str(path), url=snapshot['url'], error=snapshot['error']))
            continue
        fetched = snapshot['fetched']
        assert sha(fetched['html'].encode()) == snapshot['html_hash']
        row = compare(path.stem, fetched['url'], fetched['html'], source_text)
        row.update(snapshot=str(path), requested_url=snapshot['url'], http_status=fetched['status'])
        rows.append(row)
    add_gold_quote_coverage(rows)
    control = Path('tests/fixtures/extract/git-clean.html')
    development = compare('development-git-clean', 'https://git-scm.com/docs/git-clean', control.read_text(), source_text)
    assert development['branches']['baseline']['exit_code'] == development['branches']['fixed']['exit_code'] == 0
    assert development['branches']['fixed']['content_scope'] == 'declared_body'
    assert all(development['branches']['fixed']['flags'].values())
    result = dict(protocol=dict(date='2026-10-30', mode='offline CLI extract-file on identical retained HTML',
                               scope='all captured raw HTML; acquisition failures listed separately; git-clean fixture is historical development control, not fresh query quality',
                               limitation='more extracted characters do not establish relevance, completeness or correctness; no HTTP/status/navigation validation is re-run by extract-file; no network or models'),
                  binaries=binaries, source_outcomes=len(rows) + len(failed_acquisitions), html_inputs=len(rows),
                  acquisition_failures=failed_acquisitions, same_extracted_text=sum(r['same_extracted_text'] is True for r in rows),
                  changed_extracted_text=sum(r['same_extracted_text'] is False for r in rows),
                  extraction_errors={branch: sum(r['branches'][branch]['exit_code'] != 0 for r in rows) for branch in BINARIES},
                  declared_body_inputs=sum(r['pagefind_body_count'] > 0 for r in rows),
                  method_counts={branch: dict(collections.Counter(r['branches'][branch].get('extraction_method', 'error') for r in rows)) for branch in BINARIES},
                  rows=rows, development_control=development)
    dump(DEST, result)
    print(json.dumps({key: value for key, value in result.items() if key not in ('rows', 'development_control', 'binaries')}, ensure_ascii=False, indent=2))
    print('development-git-clean', {b: development['branches'][b] for b in BINARIES})


if __name__ == '__main__':
    main()
