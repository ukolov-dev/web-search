"""Blind source gold. Reads retrieval cards and source HTML only, never predictions."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/direct-coverage-2026-10-30')
# Filled by source-only inspection before either branch's labels are disclosed.
SPECS = {
    'c01': {'C009': ['This flag applies the changes necessary to cherry-pick each named commit to your working tree and the index, without making any commit.',
                     'your index does not have to match the HEAD commit.',
                     'The cherry-pick is done against the beginning state of your index.',
                     "This is useful when cherry-picking more than one commits' effect to your index in a row."]},
    'c02': {'C005': ['function :: asdict(obj, *, dict_factory=dict)',
                     'dataclasses, dicts, frozendicts, lists, and tuples are recursed into.',
                     'Other objects are copied with :func: ` copy.deepcopy `.',
                     "assert asdict(c) == {'mylist': [{'x': 0, 'y': 0}, {'x': 10, 'y': 4}]}" ]},
    'c03': {'C005': ['When the last Rc pointer to a given allocation is destroyed, the value stored in that allocation (often referred to as “inner value”) is also dropped.',
                     'this will return None if the value stored in the allocation has already been dropped.',
                     'Weak pointers do not keep the value inside the allocation alive; however, they do keep the allocation (the backing store for the inner value) alive.'],
            'C007': ['a Weak reference does prevent the allocation itself (the backing store) from being deallocated.',
                     'Returns None in the following cases: The inner value has since been dropped or moved out.',
                     'Destroy all strong pointers. drop(strong_five); drop(five); assert! (weak_five.upgrade().is_none());'],
            'C018': ['When the last Rc pointer to a given allocation is destroyed, the value stored in that allocation (often referred to as “inner value”) is also dropped.',
                     'this will return None if the value stored in the allocation has already been dropped.',
                     'Weak pointers do not keep the value inside the allocation alive; however, they do keep the allocation (the backing store for the inner value) alive.']},
    'c04': {'C005': ['NOT IN , which is semantically different since its logic is trivalent and it can return NULL',
                     'this condition is semantically equivalent to NOT EXISTS (but not to NOT IN )'],
            'C016': ['NOT IN with NULLs returns no rows.',
                     'SQL’s three-valued logic treats NULL as “unknown”, and NOT IN with an unknown returns unknown.',
                     'The fix: use NOT EXISTS:',
                     'NOT EXISTS is NULL-safe'],
            'C017': ['WHERE x NOT IN (SELECT y …) returns zero rows the moment the subquery produces any NULL, because x <> NULL evaluates to NULL (unknown), not true',
                     'Fix 1 — NOT EXISTS: correlated subquery, NULL-safe by design']},
    'c05': {'C014': ['The ADD COLUMN syntax is used to add a new column to an existing table.',
                     'If a NOT NULL constraint is specified, then the column must have a default value other than NULL.',
                     'No changes are made to table content for renames or column addition without constraints.']},
    'c06': {'C005': ['Build arguments declared in the global scope of a Dockerfile aren\'t automatically inherited into the build stages.',
                     "Once a build argument is declared or consumed in a stage, it's automatically inherited by child stages.",
                     'Environment variables are passed through to the build execution environment, and persist in containers instantiated from the image.']},
    'c07': {'C007': ['Using order changes the order in which items are painted, and the order in which they appear visually. It does not change the sequential navigation order of the items.',
                     'The tabbing order and speech order follow the order of the source code.'],
            'C011': ['The grid and flexbox specifications detail that reordering caused by these methods is only visual, and note that use of order or grid placement to do logical ordering is non-conforming.',
                     'if the author has reordered content using grid or flexbox, then the tab order of the document for keyboard users who are referencing the visual display can become disconnected, as it follows the document order.'],
            'C015': ['While the order property affects the visual sequence of flex items, it does not update their arrangement in the source code.',
                     'when a user presses the Tab key to navigate between each button, the “Update tags” button will still receive focus before “Update description,” even though they’re displayed in reverse order on the screen.'],
            'C016': ['With flexbox it is possible to display content in a different visual order, without changing the DOM order.',
                     'When you use the tab key to move through the content, there is a disconnect between the visual order and the keyboard navigation (DOM) order.'],
            'C017': ['it allows developers to make the visual order of elements on the page different than their DOM order.',
                     'The order property does not affect ordering in non-visual media (such as speech). Likewise, order does not affect the default traversal order of sequential navigation modes']},
    'c08': {'C019': ['Clients receiving a 421 (Misdirected Request) response from a server MAY retry the request — whether the request method is idempotent or not — over a different connection.']},
}
PARTIAL = {
    'c01': {'C001': '--no-commit explained, but multiple-commit accumulation against the initial index state is not established',
            'C007': 'GitHub Desktop multi-commit cherry-pick workflow, not --no-commit or its initial-index semantics',
            'C018': '--no-commit mention without multiple-commit or initial-index behavior'},
    'c02': {'C013': 'PocketPy explicitly documents shallow asdict without recursive conversion, different implementation scope from the requested standard dataclasses behavior',
            'C018': 'recursive nested-dataclass conversion explained, but deepcopy of other objects not established'},
    'c04': {'C006': 'PostgreSQL trigger tutorial without NOT IN/NOT EXISTS relationship',
            'C010': 'generic SQL three-valued logic and NOT EXISTS alternative described; applicability to named PostgreSQL context not established in source text'},
    'c05': {'C016': 'NOT NULL DEFAULT syntax recipe, but non-NULL default requirement and existing-row behavior are not established'},
    'c06': {'C004': 'ARG build-only versus ENV runtime persistence explained; multi-stage inheritance/scoping not established',
            'C010': 'concrete multistage Dockerfile and argument redeclaration, but ARG/ENV inheritance/persistence distinction not established'},
    'c08': {'C001': 'SNI/connection mismatch and server-configuration fix, no non-idempotent retry permission',
            'C003': '421 retry and connection troubleshooting described; non-idempotent exception not established',
            'C005': 'different-connection retry described without the materially requested non-idempotent exception',
            'C009': 'concrete APISIX coalesced-connection failure and421 fix, but no non-idempotent retry condition'},
}
NEGATIVE = {
    'c01': {'C006': 'Windows development blog index, not cherry-pick index-state semantics'},
    'c02': {'C002': 'itertools crash issue, not dataclasses.asdict copying',
            'C009': 'bookstore Python data-science listing, no requested object-copy relationship',
            'C010': 'bookstore Python data-science listing, no requested object-copy relationship',
            'C011': 'Python data-science course schedule, not dataclasses.asdict behavior'},
    'c03': {'C002': 'equity research and weak financial margins, different subject',
            'C003': 'chess variant applets, unrelated dropped allocation',
            'C009': 'aircraft article, unrelated Rc allocation',
            'C010': 'package build failures, not Rc/Weak semantics',
            'C013': 'infrastructure asset-management product, not Rust pointers',
            'C017': 'cloud job-allocation changelog, different allocation subject'},
    'c04': {'C007': 'mailing-list index, not the requested NULL relationship',
            'C009': 'durable workflow SQL library, not requested predicate semantics',
            'C013': 'application-development service, not requested predicate semantics',
            'C018': 'MySQL cheat sheet, different database and no requested relationship'},
    'c05': {'C004': 'card explicitly describes Oracle/MySQL changes, not SQLite',
            'C005': 'Comdb2 SQL dialect, different database',
            'C006': 'SQLite commercial development service, no requested schema-change rule',
            'C007': 'Databricks/Delta SQL dialect, not SQLite',
            'C008': 'Snowflake SQL dialect, not SQLite',
            'C010': 'SQL Server/T-SQL dialect, not SQLite',
            'C012': 'PostgreSQL ADD COLUMN, different database'},
    'c06': {},
    'c07': {'C002': 'nonprofit home page, not CSS order behavior',
            'C003': 'retail store information, unrelated order',
            'C006': 'browser-extension storefront, no requested CSS/focus relationship',
            'C008': 'desktop window-management product comparison, unrelated order',
            'C009': 'Drupal template migration issue, unrelated CSS/focus ordering',
            'C010': 'checkout-marketing product changelog, unrelated order',
            'C012': 'map-plugin changelog, not CSS flex focus order'},
    'c08': {'C008': 'MCP reload retry bug, not HTTP421 retry rule',
            'C011': 'MCP mutation retry issue, not HTTP421 retry rule',
            'C012': 'Temporal payload-codec retry feature request, not HTTP421 retry rule',
            'C016': 'LinkedIn protocol-topic listing, no HTTP421 retry behavior'},
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
    dest = Path('feasibility/direct-coverage-gold-v1.json')
    assert not dest.exists(), 'gold already frozen; never relabel after predictions inspected'
    pool = load(ROOT / 'retrieval/corpus.json')
    assert len(pool['cases']) == 8
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
                             normalized_text_hash=sha(text.encode()), source_chars=len(text), final_url=final_url)
                if ident in PARTIAL.get(cid, {}):
                    entry.update(grade=1, basis=PARTIAL[cid][ident])
                for needle in SPECS.get(cid, {}).get(ident, []):
                    match = re.search(re.escape(needle), text, re.I)
                    assert match, (cid, ident, needle)
                    start, end = match.span()
                    entry['support'].append(dict(quote=text[start:end], start=len(text[:start].encode()),
                                                 end=len(text[:end].encode())))
                if entry['support']:
                    entry.update(grade=2, basis='concrete requested relationship and material conditions in inspected retained source')
                if cid == 'c07' and ident == 'C017' and entry['grade'] == 2:
                    entry['scope_warning'] = 'positive applies to quoted normative CSS order/traversal relationship; historical Firefox bug/workaround claims are not validated as current'
                if cid == 'c07' and ident == 'C011' and entry['grade'] == 2:
                    entry['scope_warning'] = 'positive applies only to explicit existing visual-versus-tab/document-order behavior in the issue introduction; rejected/new visual-order or reading-order proposals are not treated as shipped features'
                if cid == 'c04' and ident == 'C005' and entry['grade'] == 2:
                    entry['scope_warning'] = 'positive applies to explicit PostgreSQL NULL/NOT IN/NOT EXISTS semantics, corroborated by current acquired c04 sources; PostgreSQL8.4 execution-plan/performance advice is not validated as current'
                if cid == 'c08' and ident == 'C019' and entry['grade'] == 2:
                    entry['scope_warning'] = 'positive applies to the requested retry condition only; unrelated cacheability statements and legacy RFC references are not evaluated as current'
                if cid == 'c02' and ident == 'C006' and 'View remainder of file in raw view' in text:
                    entry['inspection_limit'] = 'captured GitHub HTML ends before the asdict implementation; not evidence that the full source file is irrelevant'
            if 'mate.naver.com' in candidate['url']:
                entry.update(grade=0, basis='partner navigation card, unrelated source')
            if ident in NEGATIVE.get(cid, {}):
                entry.update(grade=0, basis=NEGATIVE[cid][ident])
            gold.append(entry)
        rows.append(dict(id=cid, query=row['case']['query'], gold=gold))
    result = dict(protocol=dict(author='one separate source annotator, blinded to model outputs/branch labels; not independent acceptance',
                               freeze='queries/code fixed before inference; source gold fixed after acquisition and before prediction inspection',
                               scope='known relevant retrieved sources, not universal web recall; null unjudged and unknown direct separately reported',
                               source_gold='retained original HTML from raw acquisition union and preselected source-only audits; never branch-extracted text',
                               criteria='grade2 concrete requested relationship with material conditions;grade1 partial/unresolved/wrong factual condition only with concrete evidence;grade0 unrelated;short challenge/unobserved source unknown',
                               proposals='status/genre alone is insufficient to reject source; unresolved conjecture is not current-behavior evidence, concrete current behavior remains assessable',
                               offsets='UTF-8 bytes in whitespace-normalized source_text(original HTML); scripts/styles/templates/noscript/svg excluded',
                               isolation='annotator did not read branch results, model calls, labels or logs; gold-only audits never injected into model inputs'),
                  cases=rows)
    dest.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    paths = [dest, Path('feasibility/direct-coverage-cases-v1.json'), ROOT / 'retrieval/corpus.json']
    (ROOT / 'gold-freeze.json').write_text(json.dumps({str(p): sha(p.read_bytes()) for p in paths}, indent=2) + '\n')
    print('known', sum(g['grade'] is not None for r in rows for g in r['gold']),
          'positive', sum(g['grade'] == 2 for r in rows for g in r['gold']))


if __name__ == '__main__':
    build()
