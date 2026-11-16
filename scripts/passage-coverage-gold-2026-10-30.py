"""Blind source-only annotation. Never reads branch outputs, model calls or labels."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/passage-coverage-2026-10-30')
# Source-only criteria fixed before any new model outputs are inspected.
# Filled from retained original HTML; absent source stays unjudged.
SPECS = {'p01': {'C004': ['Path. absolute ( ) ¶ Make the path absolute, without normalization or resolving symlinks.',
                  'Path. resolve ( strict = False ) ¶ Make the path absolute, resolving any symlinks.',
                  'If strict is False , the path is resolved as far as possible and any remainder is '
                  'appended without checking whether it exists.']},
 'p02': {'C003': ['When the --soft argument is passed, the ref pointers are updated and the reset stops '
                  'there. The Staging Index and the Working Directory are left untouched.',
                  "'--mixed This is the default operating mode. The ref pointers are updated. The Staging "
                  'Index is reset to the state of the specified commit. Any changes that have been undone '
                  'from the Staging Index are moved to the Working Directory.',
                  'When passed --hard The Commit History ref pointers are updated to the specified commit. '
                  'Then, the Staging Index and Working Directory are reset to match that of the specified '
                  'commit.'],
         'C004': ['git reset moves HEAD back to an earlier commit;',
                  'Mode Moves branch Resets staging Resets working tree --soft Yes No No --mixed (default) '
                  'Yes Yes No --hard Yes Yes Yes'],
         'C006': ['git reset [ <mode> ] <commit> changes which commit HEAD points to.',
                  '--mixed Leave your working directory unchanged. Update the index to match the new HEAD , '
                  'so nothing will be staged.',
                  '--soft Leave your working tree files and the index unchanged.',
                  '--hard Overwrite all files and directories with the version from <commit> , and may '
                  'overwrite untracked files. Tracked files not in <commit> are removed so that the working '
                  'tree matches <commit> . Update the index to match the new HEAD , so nothing will be '
                  'staged.'],
         'C008': ['Mode Branch ( HEAD ) Staging area (index) Working tree (files on disk) --soft moves to '
                  'target commit unchanged unchanged --mixed moves to target commit reset to match target '
                  'unchanged --hard moves to target commit reset to match target reset to match target']},
 'p06': {'C005': ['When you bind mount a path that itself contains mounts, those submounts are also included '
                  'in the bind mount by default.',
                  'If the bind mount is read-only, the Docker Engine makes a best-effort attempt at making '
                  'the submounts read-only as well. This is referred to as recursive read-only mounts.',
                  'Recursive read-only mounts require Linux kernel version 5.12 or later.',
                  "If you're running an older kernel version, submounts are automatically mounted as "
                  'read-write by default.',
                  'Attempting to set submounts to be read-only on a kernel version earlier than 5.12, using '
                  'the bind-recursive=readonly option, results in an error.']},
 'p07': {'C001': ['sticks to its nearest ancestor that has a scrolling mechanism (created when overflow is '
                  "hidden, scroll, auto, or overlay), even if that ancestor isn't the nearest actually "
                  'scrolling ancestor'],
         'C004': ['Note that a sticky element "sticks" to its nearest ancestor that has a "scrolling '
                  'mechanism" (created when overflow is hidden , scroll , auto , or overlay ), even if that '
                  "ancestor isn't the nearest actually scrolling ancestor."],
         'C007': ['Scrolling the viewport on http://output.jsbin.com/yaxovep in any of Chrome, Firefox, or '
                  'Safari shows that the sticky element is sticking to the overflow:hidden ancestor for both '
                  "scrollers, even in the left-hand case where the content area isn't overflowed.",
                  'The auto value even creates a scrolling mechanism when not overflowing its content area.',
                  'Position was rewritten and now properly refers to the Overflow spec for its definitions, '
                  'so this is moot for Position itself. CSSOM-View has also been subsequently edited to '
                  'remove that incorrect definition.'],
         'C009': ['Sticky sticks to the nearest ancestor that scrolls (or could scroll). overflow: hidden '
                  'makes the parent that ancestor even though it never scrolls, so nothing visible happens.',
                  'Check for a missing top , an ancestor with overflow: hidden or auto , a flex parent '
                  'stretching the element, or a parent no taller than the element.']},
 'p09': {'C002': ['Currently, decorating a coroutine with cached_property would cache the coroutine itself. '
                  'But this is not useful in any way since a coroutine cannot be awaited multiple times.',
                  'class Foo: @functools.cache async def go(self): print(1) async def main(): foo = Foo() '
                  'await foo.go() await foo.go() Although now I just noticed this actually does not work '
                  'either.',
                  "Let's start from async functions support by `functools.lru_cache`.",
                  'def reawaitable(func): @wraps(func) def wrapper(*args, **kwargs): return '
                  'CachedAwaitable(func(*args, **kwargs)) return wrapper It can be combined with lru_cache '
                  'and cached_property any third-party caching decorator.',
                  '@lru_cache() @reawaitable async def coro(...): ...']},
 'p10': {'C006': ['all files under the specified directories (at any depth) will be included in the sparse '
                  'checkout, as well as files that are siblings of either the given directory or any of its '
                  'ancestors',
                  'git sparse-checkout set A/B/C sets the directory A/B/C as a recursive pattern, the '
                  'directories A and A/B are added as parent patterns.'],
         'C009': ['The set subcommand modifies the sparse-checkout file with patterns to match the files in '
                  'the given directories. Further, any files that are immediately in a directory that’s a '
                  'parent to a specified directory are also included.',
                  'For example, if you ran git sparse-checkout set A/B , then Git would include files with '
                  'names A/B/C.txt (immediate child of A/B ) and A/D.txt (immediate sibling of A/B ) as well '
                  'as E.txt (immediate sibling of A ).',
                  'Given a file path such as /A/B/C/D.txt , Git includes the path in the working directory '
                  'if any of the following are true: A/B/C is in the parent set, A/B/C is in the recursive '
                  'set, A/B is in the recursive set, or A is in the recursive set.']},
 'p11': {'C007': ['if a WITH query is non-recursive and side-effect-free (that is, it is a SELECT containing '
                  'no volatile functions) then it can be folded into the parent query, allowing joint '
                  'optimization of the two query levels. By default, this happens if the parent query '
                  'references the WITH query just once, but not if it references the WITH query more than '
                  'once.',
                  'You can override that decision by specifying MATERIALIZED to force separate calculation '
                  'of the WITH query, or by specifying NOT MATERIALIZED to force it to be merged into the '
                  'parent query.'],
         'C014': ['A WITH query that is referenced more than once in FROM is computed only once, unless '
                  'specified otherwise with NOT MATERIALIZED .',
                  'a WITH query can be marked NOT MATERIALIZED to remove this guarantee. In that case, the '
                  'WITH query can be folded into the primary query much as though it were a simple sub- '
                  "SELECT in the primary query's FROM clause.",
                  'By default, a side-effect-free WITH query is folded into the primary query if it is used '
                  "exactly once in the primary query's FROM clause.",
                  'However, such folding can be prevented by marking the WITH query as MATERIALIZED .']},
 'p12': {'C004': ['Invoking clone on Arc produces a new Arc instance, which points to the same allocation on '
                  'the heap as the source Arc , while increasing a reference count.',
                  'Makes a clone of the Arc pointer. This creates another pointer to the same allocation, '
                  'increasing the strong reference count.']},
 'p04': {'C015': ['Table owners normally bypass row security as well, though a table owner can choose to be '
                  'subject to row security with ALTER TABLE ... FORCE ROW LEVEL SECURITY .']},
 'p03': {'C002': ['/// Returns the bounds on the remaining length of the iterator. fn size_hint ( & self ) '
                  '-> ( usize , Option < usize >) { .. }',
                  'A buggy iterator may yield less than the lower bound or more than the upper bound of '
                  'elements.',
                  'pub trait ExactSizeIterator : Iterator { /// Returns the exact remaining length of the '
                  'iterator. fn len ( & self ) -> usize { .. }',
                  'ExactSizeIterator has the same memory-safety guarantees as Iterator::size_hint meaning: '
                  "you can't rely on it to be correct."]}}
PARTIAL = {'p01': {'C002': 'proposed _PathBase.resolve implementation uses absolute/readlink, but no concrete complete '
                 'strict=False missing-path and absolute comparison',
         'C003': 'source code uses absolute for ancestor checks then resolve for symlink checks; no '
                 'strict=False missing-remainder semantics',
         'C007': 'concrete missing-path examples and a current resolve code comment, but no complete resolve '
                 'versus absolute comparison; historic bug report does not establish a current bug'},
 'p03': {'C004': 'concrete exact-length size_hint obligation and safe-trait caveat, but no description of '
                 'the lower and optional upper tuple bounds',
         'C006': 'complete size_hint bounds and reliability explanation, but ExactSizeIterator only appears '
                 'in rposition signatures, not an exact-length explanation',
         'C012': 'concrete size_hint limitations and a proposal for new size wrappers; does not completely '
                 'establish the existing traits bounds and guarantee relationship',
         'C020': 'size_hint bounds and reliability explanation, but ExactSizeIterator only appears in '
                 'rposition signatures; versioned historical mirror is not proof of current API changes'},
 'p04': {'C010': 'describes table owner bypass and FORCE/NO FORCE mapping in a proposed SQLAlchemy API; does '
                 'not state PostgreSQL owner bypass default',
         'C018': 'describes Supabase bypassrls and a FORCE recursion caveat, but does not explain the '
                 'PostgreSQL table-owner default and FORCE exception'},
 'p05': {'C001': 'explains excluded new values and generic INSERT OR ABORT statement rollback, but does not '
                 'state DO UPDATE always uses ABORT',
         'C005': 'concrete SQLite DO UPDATE ABORT failure and excluded usage in a Turso defect report, but '
                 'excluded value meaning and whole-statement rollback are not completely explained'},
 'p06': {'C010': 'proposes Docker rro flag and describes existing runc recursive behavior; does not '
                 'establish current Docker bind-recursive behavior',
         'C013': 'explains recursive readonly mount_setattr and Linux 5.12 kernel dependency in Kubernetes, '
                 'but no Docker bind mount versus submount behavior'},
 'p08': {'C004': 'gives generic Content-Length resource size definition; no 304 selected-200 length '
                 'condition',
         'C007': 'explicit no-body and equivalent 200 response headers, but does not state the '
                 'Content-Length equality condition'},
 'p09': {'C005': 'explains async def coroutine functions, but no functools caching or coroutine reuse '
                 'behavior',
         'C016': 'explains standard lru_cache memoization and sync example, but no async coroutine object or '
                 'await reuse limitation'},
 'p10': {'C010': 'concrete sparse-checkout cone setup and compatibility tests, but no demonstrated complete '
                 'selected-subtree versus parent-directory-file inclusion relationship'},
 'p11': {'C004': 'current post excerpt describes side-effect-free single/multiple reference defaults and '
                 'names both clauses, but does not explain which clause forces folding versus separate '
                 'calculation'},
 'p12': {'C005': 'explicit Arc smart-pointer clone shared-data explanation, but no increment of the strong '
                 'reference count',
         'C014': 'concrete Arc shared-pointer and reference-counting discussion, but no complete clone '
                 'allocation-sharing and strong-count increment relationship'}}
NEGATIVE = {'p01': {'C013': 'retrieval card explicitly describes a scheduled repository reviewer, not pathlib path '
                 'resolution',
         'C015': 'retrieval card explicitly describes Jinja expression resolution, not filesystem '
                 'Path.resolve',
         'C017': 'retrieval card explicitly describes sysconfig build and installation configuration, not '
                 'pathlib path resolution',
         'C019': 'retrieval card explicitly describes TensorFlow path_to_str conversion, not pathlib resolve '
                 'versus absolute'},
 'p03': {'C010': 'retrieval card explicitly compares string functions across languages, not iterator size '
                 'bounds'},
 'p04': {'C006': 'retrieval card explicitly describes BigQuery column-level access control, not PostgreSQL '
                 'owner row-security behavior',
         'C011': 'retrieval card explicitly describes SQL Server row-level security, not PostgreSQL owner '
                 'FORCE behavior',
         'C012': 'retrieval card explicitly identifies a partner-navigation shortcut, unrelated to the '
                 'requested technical relationship'},
 'p05': {'C008': 'retrieval card explicitly describes SQL Server ALTER DATABASE compatibility levels, not '
                 'SQLite UPSERT',
         'C009': 'retrieval card explicitly identifies a partner-navigation shortcut, unrelated to the '
                 'requested technical relationship'},
 'p08': {'C008': 'retrieval card explicitly describes ExtendSim product enhancements, not HTTP response '
                 'semantics',
         'C010': 'retrieval card explicitly describes FunnelKit product updates, not HTTP response semantics',
         'C012': 'retrieval card explicitly describes a vehicle diagnostic cable, not HTTP response '
                 'semantics',
         'C016': 'retrieval card explicitly describes a securities registration statement, not HTTP response '
                 'semantics',
         'C017': 'retrieval card explicitly describes beauty retail, not HTTP response semantics'},
 'p12': {'C002': 'retrieval card explicitly describes CAD utilities downloads, not Rust Arc clone semantics',
         'C012': 'retrieval card explicitly describes product integrations, not Rust Arc clone semantics',
         'C013': 'retrieval card explicitly describes an Airbnb clone application, not Rust Arc clone '
                 'semantics',
         'C018': 'retrieval card explicitly describes Synthiam ARC software releases, not Rust Arc clone '
                 'semantics'},
 'p06': {'C015': 'retrieval card explicitly identifies a partner-navigation shortcut, unrelated to the '
                 'requested technical relationship'},
 'p09': {'C013': 'retrieval card explicitly identifies a partner-navigation shortcut, unrelated to the '
                 'requested technical relationship'},
 'p10': {'C016': 'retrieval card explicitly identifies a partner-navigation shortcut, unrelated to the '
                 'requested technical relationship'},
 'p11': {'C013': 'retrieval card explicitly identifies a partner-navigation shortcut, unrelated to the '
                 'requested technical relationship'}}
SOURCE_NEGATIVE = {'p05': {'C011': 'inspected body enumerates SQLite extended result-code constants and API return codes; no '
                 'UPSERT or excluded conflict-update semantics'},
 'p01': {'C016': 'inspected body is symbol-name index entries including pathlib absolute and resolve, with '
                 'no path resolution behavior or strict=False condition'}}
SCOPE_WARNINGS = {'p06': {'C001': 'retained HTML exposes only a 26-character blog wrapper; article content unobserved'},
 'p08': {'C003': 'retained HTML exposes only a 24-character blog wrapper; article content unobserved'},
 'p10': {'C011': 'retained response explicitly says 404 and requested file absent from branch; source '
                 'content unobserved',
         'C012': 'retained HTML is only GitLab file heading and Loading; file content unobserved'},
 'p11': {'C001': 'retained HTML exposes only a 28-character blog wrapper; article content unobserved'}}
CRITERIA = {
    'p01': 'resolve(strict=False) makes an absolute path, resolves symlinks and handles a missing remainder without error; distinguishes absolute().',
    'p02': 'soft, mixed and hard distinguish effects on HEAD, index and working tree.',
    'p03': 'size_hint lower and optional upper bounds, ExactSizeIterator exact remaining length, and the relevant reliability qualifications.',
    'p04': 'table owner bypasses row security by default and FORCE ROW LEVEL SECURITY subjects the owner to it.',
    'p05': 'excluded is attempted insert values; DO UPDATE uses ABORT and rolls back the entire INSERT statement, not necessarily the transaction.',
    'p06': 'read-only bind mount submount behavior and recursive read-only Linux kernel 5.12 condition.',
    'p07': 'sticky attaches to the nearest ancestor with a scrolling mechanism such as hidden or auto, even if that ancestor does not actually scroll.',
    'p08': '304 has no message body; any Content-Length must equal the selected 200 response representation length.',
    'p09': 'standard functools.lru_cache caches a coroutine object for an async function, creating an await reuse limitation.',
    'p10': 'cone mode recursively includes selected directories and files in parent or leading directories, rather than every ancestor subtree.',
    'p11': 'side-effect-free CTE folding versus default materialization when multiply referenced, and MATERIALIZED/NOT MATERIALIZED overrides.',
    'p12': 'Arc clone shares the allocation and value and increments the strong reference count rather than deep-copying the value.',
}

class SourceText(HTMLParser):
    def __init__(self):
        super().__init__()
        self.hidden = []
        self.parts = []
        self.apps = []
        self.code_scripts = []
        self.script = None

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'react-app':
            self.apps.append(attrs.get('app-name') == 'code-view')
        if tag == 'script' and any(self.apps) and attrs.get('type') == 'application/json' and attrs.get('data-target') == 'react-app.embeddedData':
            self.script = []
        if tag in ('script', 'style', 'template', 'noscript', 'svg'):
            self.hidden.append(tag)

    def handle_endtag(self, tag):
        if tag == 'script' and self.script is not None:
            self.code_scripts.append(''.join(self.script))
            self.script = None
        if self.hidden and self.hidden[-1] == tag:
            self.hidden.pop()
        if tag == 'react-app' and self.apps:
            self.apps.pop()

    def handle_data(self, data):
        if self.script is not None:
            self.script.append(data)
        if not self.hidden:
            self.parts.append(data)


def source_text(html):
    parser = SourceText()
    parser.feed(html)
    # A publisher-declared JSON file is source data, not executable script or model evidence.
    if len(parser.code_scripts) == 1:
        try:
            payload = json.loads(parser.code_scripts[0])['payload']
            route = payload['codeViewBlobLayoutRoute']
            lines = payload['codeViewBlobLayoutRoute.StyledBlob']['rawLines']
            if route['path'] == payload['codeViewLayoutRoute']['path'] and isinstance(lines, list) and all(isinstance(line, str) for line in lines):
                parser.parts.append('\n'.join(lines))
        except (KeyError, ValueError, TypeError):
            pass
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
    dest = Path('feasibility/passage-coverage-gold-v1.json')
    assert not dest.exists(), 'gold already frozen; never relabel after predictions inspected'
    pool = load(ROOT / 'retrieval/corpus.json')
    assert len(pool['cases']) == 12
    case_ids = {row['case']['id'] for row in pool['cases']}
    assert case_ids == set(CRITERIA)
    for labels in (SPECS, PARTIAL, NEGATIVE, SOURCE_NEGATIVE, SCOPE_WARNINGS):
        assert set(labels) <= case_ids
        for row in pool['cases']:
            cid = row['case']['id']
            assert set(labels.get(cid, {})) <= {c['id'] for c in row['candidates']}, cid
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
                if ident in SOURCE_NEGATIVE.get(cid, {}):
                    entry.update(grade=0, basis=SOURCE_NEGATIVE[cid][ident], basis_field='raw_html_text')
                if ident in SCOPE_WARNINGS.get(cid, {}):
                    entry['scope_warning'] = SCOPE_WARNINGS[cid][ident]
            if ident in NEGATIVE.get(cid, {}):
                entry.update(grade=0, basis=NEGATIVE[cid][ident], basis_field='retrieval_card')
            gold.append(entry)
        rows.append(dict(id=cid, query=row['case']['query'], criteria=CRITERIA[cid], gold=gold))
    result = dict(protocol=dict(author='one separate source annotator, blinded to model outputs/branch labels; not independent acceptance',
                               freeze='source gold fixed after acquisition and before prediction inspection; no source grades based on branch output',
                               scope='known relevant retrieved sources, not universal web recall; null unjudged and unknown direct separately reported',
                               source_gold='retained original HTML/raw acquisition and preselected source-only audits; never branch-extracted text',
                               criteria='grade2 concrete requested relationship with material conditions;grade1 partial/unresolved/wrong factual condition only with concrete counter-evidence;grade0 unrelated;unobserved/challenge/incomplete source unknown',
                               proposals='status/genre/date alone is not negative; conjecture is not shipped behavior; concrete current explanations in an issue remain assessable',
                               offsets='UTF-8 bytes in whitespace-normalized source_text(original HTML); visible text plus uniquely declared typed code-view JSON rawLines; other scripts/styles excluded',
                               isolation='annotator did not read new branch results, model calls, labels or logs; source-only audits not injected into model input'),
                  cases=rows)
    dest.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    paths = [dest, Path('feasibility/passage-coverage-cases-v1.json'), ROOT / 'retrieval/corpus.json']
    (ROOT / 'gold-freeze.json').write_text(json.dumps({str(p): sha(p.read_bytes()) for p in paths}, indent=2) + '\n')
    print('known', sum(g['grade'] is not None for r in rows for g in r['gold']),
          'positive', sum(g['grade'] == 2 for r in rows for g in r['gold']))


if __name__ == '__main__':
    build()
