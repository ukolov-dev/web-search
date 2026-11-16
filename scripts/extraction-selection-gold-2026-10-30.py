"""Blind source gold: never reads branch results, predictions or model calls."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/extraction-selection-2026-10-30')
# Filled from source-only inspection, before predicted labels are disclosed.
SPECS = {
    'n01': {'C011': ['can fail with conflicts', 'not removed from the stash list',
                     'but do not remove the state']},
    'n02': {'C001': ['returns consecutive keys and groups',
                     'shares the underlying iterable with groupby()',
                     'the previous group is no longer visible']},
    'n03': {'C002': ['fn read_until', 'fn read_line',
                     'including, the delimiter (if found)',
                     'error if the read bytes are not valid UTF-8']},
    'n04': {'C011': ['without the ORDER BY clause, the “first” unique entry becomes unpredictable'],
            'C020': ['the “first row” of a set is unpredictable unless the query is sorted on enough columns']},
    'n05': {'C013': ['delete triggers fire if and only if recursive triggers are enabled.',
                     'The update hook is not invoked',
                     'Nor does REPLACE increment the change counter']},
    'n06': {'C007': ['empty volume',
                     'files or directories are propagated (copied) into the volume by default',
                     'use the volume-nocopy option']},
    'n07': {'C007': ['The difference between clip and hidden is that the clip keyword also forbids all scrolling, including programmatic scrolling.',
                     'not a scroll container.'],
            'C009': ['スクロールバーのない スクロールコンテナー となります',
                     'scrollTo() メソッドなどの他の方法によるスクロールはできます。',
                     '要素ボックスはスクロールコンテナーにはならず、切り取られたコンテンツは表示されず、プログラムによるスクロールは対応していません。']},
    'n08': {'C007': ['Content-Length header to check the file size',
                     "server shouldn't include a message body in the response"],
            'C009': ['HEAD response must not include a message body.',
                     'processed like GET but returns only the headers', 'Content-Length'],
            'C011': ['HEAD responses MUST NOT include a response body',
                     'Content-Length indicating what the equivalent GET response would return'],
            'C016': ['GET 메서드와 거의 똑같지만, 서버가 응답 본문(Response Body)을 보내지 않고 헤더(Headers)만',
                     'Content-Length (콘텐츠 크기)']},
}
PARTIAL = {
    'n01': {'C001': 'pop/apply and conflicts explained, but no witness that a conflicting pop retains the stash entry',
            'C009': 'pop/apply command summary without conflict-retention semantics',
            'C015': 'pop/apply and branch recovery described; conflicting-pop retention not established',
            'C020': 'source_conflict: wrong material condition; says conflicting pop removes stash, contradicted by official Git retained-state rule'},
    'n02': {'C002': 'consecutive groups explained; shared underlying iterator lifetime not established',
            'C018': 'concrete groupby concatenation example; shared underlying iterator lifetime not established'},
    'n03': {'C007': 'pre-stabilization IO reform proposal with historical method signatures, not current BufRead contract',
            'C008': 'pre-stabilization IO reform proposal with historical method signatures, not current BufRead contract',
            'C013': 'pre-stabilization IO reform proposal with historical method signatures, not current BufRead contract'},
    'n04': {'C006': 'UNION/INTERSECT/EXCEPT ordering, not DISTINCT ON first-row selection',
            'C008': 'ORDER BY selecting a group representative explained, but absent/insufficient ordering unpredictability not established'},
    'n05': {'C006': 'library changelog and dialect features, no requested REPLACE behavioral exceptions',
            'C018': 'trigger experiments include INSERT OR REPLACE, but recursive setting/update-hook/change-counter exceptions not established'},
    'n06': {'C008': 'nocopy key appears in application config, but empty-volume copy/default suppression relationship not explained',
            'C012': 'volume initialization is explained and nocopy appears in an example, but its effect is not established by source prose',
            'C006': 'Ansible Docker no_copy option described; empty-volume condition is not established'},
    'n07': {'C014': 'single-axis scroll-snap propagation issue, not hidden-versus-clip programmatic scrolling',
            'C015': 'historical design review of clip; comparison with hidden is unresolved and design marked unsatisfied'},
}
NEGATIVE = {
    'n01': {'C006': 'Git GUI product overview, not stash conflict semantics',
            'C012': 'different stash: key-value library, not Git'},
    'n02': {'C007': 'TypeScript port rather than Python shared iterator semantics',
            'C010': 'TypeScript/JavaScript package, not Python itertools',
            'C014': 'sorting lists, not groupby shared iterator',
            'C016': 'guitar synthesis, no requested groupby relationship',
            },
    'n03': {'C001': 'BufWriter writes buffers; query concerns BufRead reading',
            'C004': 'BorrowedFd descriptor ownership, not BufRead delimiters',
            'C005': 'C++11 language overview, different language/API',
            'C006': 'Vim configuration gist, not BufRead',
            'C009': 'integer division library, unrelated operation',
            'C010': 'OpenBSD clock daemon manual, unrelated operation'},
    'n04': {'C004': 'BigQuery SQL dialect, not PostgreSQL DISTINCT ON',
            'C005': 'Spanner SQL dialect, not PostgreSQL DISTINCT ON'},
    'n05': {'C002': 'MySQL/Oracle trigger tutorial, not SQLite REPLACE exceptions',
            'C003': 'Oracle database triggers, not SQLite',
            'C004': 'Oracle PL/SQL triggers, not SQLite',
            'C009': 'PostgreSQL extension search, not SQLite',
            'C010': 'SQL Server trigger semantics, not SQLite',
            'C019': 'Swift language cheat sheet, unrelated'},
    'n06': {'C004': 'CISA security bulletin, no Docker volume copy behavior',
            'C016': 'Skeema schema options, not Docker volume copy behavior'},
    'n07': {'C008': 'CSS selectors reference, different CSS operation'},
    'n08': {'C001': 'consumer privacy PDF, not HTTP HEAD semantics',
            'C004': 'status-code catalog, not HEAD Content-Length relationship',
            },
}


class SourceText(HTMLParser):
    def __init__(self):
        super().__init__()
        self.hidden = []
        self.parts = []

    def handle_starttag(self, tag, attrs):
        if tag in ('script', 'style', 'template', 'noscript', 'svg'):
            self.hidden.append(tag)

    def handle_endtag(self, tag):
        if self.hidden and self.hidden[-1] == tag:
            self.hidden.pop()

    def handle_data(self, data):
        if not self.hidden:
            self.parts.append(data)


def source_text(html):
    parser = SourceText()
    parser.feed(html)
    return ' '.join(' '.join(parser.parts).split())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def snapshot(case, candidate):
    path = ROOT / 'raw-cache' / (sha(candidate['url'].encode()) + '.json')
    if path.exists():
        saved = load(path)
        assert saved['url'] == candidate['url']
        if saved.get('fetched'):
            html = saved['fetched']['html']
            assert sha(html.encode()) == saved['html_hash']
            return path, html, saved['fetched']['url']
    path = ROOT / 'gold-audit' / f"{case}-{candidate['id']}.html"
    if path.exists():
        return path, path.read_text(), candidate['url']
    return None, None, None


def build():
    dest = Path('feasibility/extraction-selection-gold-v1.json')
    assert not dest.exists(), 'gold already frozen; no relabel after prediction inspection'
    pool = load(ROOT / 'retrieval/corpus.json')
    rows = []
    for row in pool['cases']:
        cid = row['case']['id']
        gold = []
        for candidate in row['candidates']:
            ident = candidate['id']
            path, html, final_url = snapshot(cid, candidate)
            entry = dict(id=ident, url=candidate['url'], grade=None,
                         basis='unjudged: no sufficient inspected source evidence', support=[])
            if html is not None:
                text = source_text(html)
                entry.update(snapshot=str(path), field='raw_html_text', html_hash=sha(html.encode()),
                             normalized_text_hash=sha(text.encode()), source_chars=len(text),
                             final_url=final_url)
                if ident in PARTIAL.get(cid, {}):
                    entry.update(grade=1, basis=PARTIAL[cid][ident])
                for needle in SPECS.get(cid, {}).get(ident, []):
                    match = re.search(re.escape(needle), text, re.I)
                    assert match, (cid, ident, needle)
                    start, end = match.span()
                    quote = text[start:end]
                    entry['support'].append(dict(quote=quote, start=len(text[:start].encode()),
                                                 end=len(text[:end].encode())))
                if entry['support']:
                    entry.update(grade=2, basis='concrete requested source relationship and material queried conditions, judged from retained original HTML')
            if 'mate.naver.com' in candidate['url']:
                entry.update(grade=0, basis='Naver partner navigation card, unrelated source')
            if cid == 'n04' and ident == 'C016':
                entry.update(grade=1, basis='retrieved card explicitly labels PostgreSQL7.0 unsupported; no historical version requested')
            if ident in NEGATIVE.get(cid, {}):
                entry.update(grade=0, basis=NEGATIVE[cid][ident])
            if cid == 'n01' and ident == 'C020' and html is not None:
                counter = next(c for c in row['candidates'] if c['id'] == 'C011')
                counter_path, counter_html, counter_url = snapshot(cid, counter)
                assert counter_html is not None, 'conflict judgement needs actual official counter-source'
                counter_text = source_text(counter_html)
                conflict = {}
                for label, quote, source, ref, url in [
                    ('claim', '충돌이 나면 stash가 사라져서 복구하기 어려울 수 있음', text, path, candidate['url']),
                    ('counter', 'not removed from the stash list', counter_text, counter_path, counter_url),
                ]:
                    start = source.index(quote)
                    conflict[label] = dict(quote=quote, start=len(source[:start].encode()),
                                           end=len(source[:start + len(quote)].encode()),
                                           snapshot=str(ref), field='raw_html_text', url=url,
                                           normalized_text_hash=sha(source.encode()))
                entry.update(source_conflict=conflict, source_relevance_grade=2,
                             warning='material factual conflict with retrieved official Git documentation; primary grade1 decided before label inspection')
            gold.append(entry)
        rows.append(dict(id=cid, query=row['case']['query'], gold=gold))
    result = dict(protocol=dict(author='one separate source annotator blinded to branch labels/model outputs; not independent acceptance',
                               freeze='queries/code fixed before inference; source gold fixed after acquisition before branch predictions inspected',
                               scope='known relevant retrieved sources, not universal web recall; null unjudged and unknown direct reported separately',
                               source_gold='original retained HTML from raw acquisition union plus preselected source-only audits; not branch-extracted text',
                               criteria='grade2 requested relationship including material conditions;grade1 partial scope;grade0 clearly unrelated;short shell/challenge/unobserved source unknown',
                               historical='unsupported/superseded version without requested historical version is grade1',
                               source_conflict='n01C020 explicitly false key condition against official n01C011:primary grade1;source_relevance_grade2 retained for separate sensitivity,not merged into primary',
                               offsets='UTF-8 bytes in whitespace-normalized source_text(original HTML); script/style/template/noscript/svg text excluded',
                               isolation='no branch output, model labels or verdicts were read by annotator; gold-only audits never injected into branch inputs'),
                  cases=rows)
    dest.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    inputs = [dest, Path('feasibility/extraction-selection-cases-v1.json'), ROOT / 'retrieval/corpus.json']
    (ROOT / 'gold-freeze.json').write_text(json.dumps({str(p): sha(p.read_bytes()) for p in inputs}, indent=2) + '\n')
    print('known', sum(g['grade'] is not None for r in rows for g in r['gold']),
          'positive', sum(g['grade'] == 2 for r in rows for g in r['gold']))


if __name__ == '__main__':
    build()
