"""Blind NEW4 source-only annotation. Never reads branch outputs, model calls or labels."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/passage-coverage-confirm-2026-10-30')
# Criteria were predeclared in gold-audit-plan.json before source inspection.
# Fill source-only exact witnesses after acquisition; missing content stays unknown.
SPECS = {'h01': {'C003': ['Return a copy of the string with trailing characters removed. The chars argument is a '
                  'string specifying the set of characters to be removed.',
                  'The chars argument is not a suffix; rather, all combinations of its values are '
                  'stripped.',
                  'See removesuffix() for a method that will remove a single suffix string rather than all '
                  'of a set of characters.',
                  'If the string ends with the suffix string and that suffix is not empty, return '
                  'string[:-len(suffix)] . Otherwise, return a copy of the original string:'],
         'C012': ['(l/r)strip : The argument is interpreted as a character set. The characters are '
                  'repeatedly removed from the appropriate end of the string.',
                  'remove(prefix/suffix) : The argument is interpreted as an unbroken substring. Only at '
                  'most one copy of the prefix/suffix is removed.',
                  "def removesuffix ( self : str , suffix : str , / ) -> str : # suffix='' should not call "
                  'self[:-0]. if suffix and self . endswith ( suffix ): return self [: - len ( suffix )] '
                  'else : return self [:]']},
 'h03': {'C001': ['Freezing an object prevents extensions and makes existing properties non-writable and '
                  'non-configurable.',
                  'function fail() { "use strict"; obj.foo = "sparky"; // throws a TypeError',
                  'If the value of those properties are objects themselves, those objects are not frozen '
                  'and may be the target of property addition, removal or value re-assignment operations.'],
         'C012': ["const foo = Object.freeze({ 'bar': 27 }); foo.bar = 42; // throws a TypeError exception "
                  'in strict mode; // silently fails in sloppy mode console.log(foo.bar); // → 27',
                  'Note that Object.freeze() is shallow: object values within a frozen object (i.e. nested '
                  'objects) can still be mutated.']},
 'h04': {'C006': ['read(2) , write(2) and lseek(2) calls all move a single pointer into the file, and '
                  'append mode, non-blocking I/O and asynchronous I/O options are shared between the '
                  'references.',
                  'The close-on-exec and close-on-fork flags on the new file descriptor are unset.',
                  'When newd equals oldd , dup2 () just returns without affecting the close-on-exec or '
                  'close-on-fork flags.'],
         'C008': ['Since the two file descriptors refer to the same open file description, they share file '
                  'offset and file status flags;',
                  'The two file descriptors do not share file descriptor flags (the close-on-exec flag).',
                  'The close-on-exec flag ( FD_CLOEXEC ; see fcntl(2) ) for the duplicate descriptor is '
                  'off.',
                  'If oldfd is a valid file descriptor, and newfd has the same value as oldfd , then dup2 '
                  '() does nothing, and returns newfd .']}}
PARTIAL = {'h01': {'C002': 'Concrete str.strip character-set behavior and exact str.removesuffix contrast, but full '
                 'retained source never names rstrip or binds the requested trailing-only operation; '
                 'both-ends strip is not the requested rstrip relation.',
         'C009': 'Concrete rstrip trailing-character behavior in Python notes; complete retained source '
                 'has no removesuffix operation or exact-suffix comparison.',
         'C014': 'Concrete strip(chars) character-set behavior, but removesuffix is absent and rstrip only '
                 'appears as a See Also link; requested two-operation trailing comparison is missing.'},
 'h02': {'C001': 'Godis Redis client source explains basic EXPIRE timeout and contains NX/XX for a '
                 'different SET operation, but no EXPIRE NX/XX/GT/LT or nonvolatile infinite-TTL option '
                 'contract; GT/LT option tokens are absent.',
         'C004': 'Full retained source documents Redict PEXPIRE options and explicitly relates '
                 'milliseconds PEXPIRE to its EXPIRE command, but does not bind those semantics to the '
                 'requested Redis EXPIRE implementation; a navigation Compatibility with Redis link is not '
                 'such evidence.',
         'C010': 'Full retained Valkey EXPIRE source explicitly supplies all '
                 'option/infinite-TTL/exclusivity semantics, but its product scope is Valkey. It does not '
                 'establish these properties for Redis EXPIRE; a Redis-code copyright/trademark footer is '
                 'not an implementation binding.'},
 'h03': {'C002': 'Actual strict-mode nonwritable-property assignment TypeError is concrete partial '
                 'support. Complete source has no Object.freeze/frozen/shallow relation; nested function '
                 'references are unrelated to nested object freezing.'},
 'h04': {'C017': 'Wrong-operation partial: this retained posix_typed_mem_open source defines initial '
                 'descriptor FD_CLOEXEC and says dup/dup2 can operate on it, but does not establish '
                 'duplicated descriptors sharing offset/status while preserving local flag state. It '
                 'explicitly marks this documentation version unmaintained; do not transfer typed-memory '
                 'creation or mmap physical-offset rules to dup.'}}
PARTIAL_SPECS = {'h01': {'C002': ['All characters in str.strip calls are removed from both the leading and trailing ends '
                  'of the string.',
                  'In Python 3.9 and later, you can use str.removeprefix and str.removesuffix to remove an '
                  'exact prefix or suffix from a string, respectively, which should be preferred when '
                  'possible.'],
         'C009': ['rstrip(self, chars=None, /) | Return a copy of the string with trailing whitespace '
                  'removed. | | If chars is given and not None, remove characters in chars instead.'],
         'C014': ['The chars argument is not a prefix; rather, all combinations of its values are '
                  'stripped.']},
 'h02': {'C001': ['//Expire Set a timeout on the specified key. After the timeout the key will be '
                  'automatically deleted by //the server. A key with an associated timeout is said to be '
                  'volatile in Redis terminology.'],
         'C004': ['This command works exactly like EXPIRE but the time to live of the key is specified in '
                  'milliseconds instead of seconds.',
                  'The PEXPIRE command supports a set of options since Redict 7.0: NX – Set expiry only '
                  'when the key has no expiry XX – Set expiry only when the key has an existing expiry GT '
                  '– Set expiry only when the new expiry is greater than current one LT – Set expiry only '
                  'when the new expiry is less than current one',
                  'A non-volatile key is treated as an infinite TTL for the purpose of GT and LT . The GT '
                  ', LT and NX options are mutually exclusive.'],
         'C010': ['A key with an associated timeout is often said to be volatile in Valkey terminology.',
                  'The EXPIRE command supports a set of options: NX -- Set expiry only when the key has no '
                  'expiry XX -- Set expiry only when the key has an existing expiry GT -- Set expiry only '
                  'when the new expiry is greater than current one LT -- Set expiry only when the new '
                  'expiry is less than current one',
                  'A non-volatile key is treated as an infinite TTL for the purpose of GT and LT . The GT '
                  ', LT and NX options are mutually exclusive.']},
 'h03': {'C002': ["Failing to assign to an object's property (e.g., it's read-only) throws a TypeError . "
                  'In sloppy mode, this would silently fail.']},
 'h04': {'C017': ['The FD_CLOEXEC file descriptor flag associated with the new file descriptor is cleared.',
                  'The mmap() , posix_mem_offset() , posix_typed_mem_get_info() , fstat() , dup() , dup2() '
                  ', and close() functions can operate on a file descriptor that posix_typed_mem_open() '
                  'returns.']}}
NEGATIVE = {}
SOURCE_NEGATIVE = {}
SCOPE_WARNINGS = {'h01': {'C003': 'Retained Python documentation includes current str methods plus historical/bytes '
                 'alternatives; proofs are from the Unicode str rstrip/removesuffix sections, not the '
                 'bytes methods.',
         'C012': 'PEP status is Final and Python-Version3.9; rejected alternatives and draft history are '
                 'not treated as current behavior.'},
 'h02': {'C005': 'Requested Redis EXPIRE original source was not acquired; no content proof and no direct '
                 'source gold.',
         'C001': 'NX/XX in SET comments cannot supply EXPIRE conditions.',
         'C004': 'Different command and product scope: Redict PEXPIRE; no source-grounded Redis EXPIRE '
                 'equivalence.',
         'C010': 'Different product scope: Valkey EXPIRE; no source-grounded Redis EXPIRE equivalence.'},
 'h04': {'C006': 'Natural close-on-exec is semantically sufficient even without literal FD_CLOEXEC; '
                 'preserve the same-descriptor exception and do not import dup3 flags.',
         'C017': 'This version of this document is no longer maintained. Source is posix_typed_mem_open, '
                 'not dup semantics; grade1 is a conservative topical partial, never direct.'}}
CRITERIA = {'h01': 'str.rstrip(chars) strips trailing characters from a character set, not one exact suffix substring; str.removesuffix(suffix) removes one exact matching suffix and otherwise leaves the string unchanged. Both operations and the comparison direction must be bound by this source.', 'h02': 'Redis EXPIRE documents the conditions for NX, XX, GT and LT; how GT/LT treat a nonvolatile key as having an infinite TTL; and which options are mutually exclusive. Do not assume all four options are mutually exclusive or transfer another command/product rule without an explicit source binding.', 'h03': 'Object.freeze is shallow: the frozen object own properties cannot be changed, while a nested referenced object remains mutable unless separately frozen. Strict mode throws TypeError for a prohibited frozen-object mutation; it does not make nested objects recursively frozen.', 'h04': 'dup/dup2 duplicates refer to the same open file description and share its file offset (and status flags), while the file-descriptor FD_CLOEXEC flag is not shared and is normally clear on a newly duplicated descriptor. Preserve any same-descriptor or platform condition stated by the source; do not confuse open-file-description state with descriptor-local flags.'}

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


def build(draft=False):
    dest = ROOT / 'gold-audit/annotation-draft.json' if draft else Path('feasibility/passage-coverage-confirm-gold-v1.json')
    assert not Path('feasibility/passage-coverage-confirm-gold-v1.json').exists(), 'gold already frozen; never relabel after predictions inspected'
    pool = load(ROOT / 'retrieval/corpus.json')
    plan = load(ROOT / 'gold-audit-plan.json')
    assert plan['protocol']['criteria'] == CRITERIA, 'predeclared material criteria changed'
    if not draft:
        assert (ROOT / 'gold-audit/cache-complete-confirmation.json').exists(), 'wait for parent CACHECOMPLETE before gold freeze'
    assert len(pool['cases']) == 4
    case_ids = {row['case']['id'] for row in pool['cases']}
    assert case_ids == set(CRITERIA)
    for labels in (SPECS, PARTIAL, PARTIAL_SPECS, NEGATIVE, SOURCE_NEGATIVE, SCOPE_WARNINGS):
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
                for needle in PARTIAL_SPECS.get(cid, {}).get(ident, []):
                    match = re.search(re.escape(needle), text, re.I)
                    assert match, (cid, ident, needle)
                    start, end = match.span()
                    entry.setdefault('partial_support', []).append(dict(quote=text[start:end], start=len(text[:start].encode()),
                                                                        end=len(text[:end].encode())))
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
            if ident in SCOPE_WARNINGS.get(cid, {}):
                entry['scope_warning'] = SCOPE_WARNINGS[cid][ident]
            if entry['grade'] == 1:
                assert entry.get('partial_support'), ('partial source proof required', cid, ident)
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
    result['protocol']['artifact_status'] = 'source-only review draft, not frozen' if draft else 'frozen before prediction inspection'
    if not draft:
        peer_path = ROOT / 'gold-audit/peer-review.json'
        assert peer_path.exists(), 'blind source peer review required before final freeze'
        result['protocol']['blind_source_review'] = dict(path=str(peer_path), sha256=sha(peer_path.read_bytes()), scope='all grade2 and grade1 sources; separate blind reviewer; not formal independent acceptance')
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    if draft:
        print('draft prepared for blind source review')
        return
    paths = [dest, Path('feasibility/passage-coverage-confirm-cases-v1.json'), ROOT / 'retrieval/corpus.json', ROOT / 'gold-audit-plan.json', ROOT / 'gold-audit/cache-complete-confirmation.json', Path(__file__), peer_path]
    (ROOT / 'gold-freeze.json').write_text(json.dumps({str(p): sha(p.read_bytes()) for p in paths}, indent=2) + '\n')
    print('known', sum(g['grade'] is not None for r in rows for g in r['gold']),
          'positive', sum(g['grade'] == 2 for r in rows for g in r['gold']))


if __name__ == '__main__':
    import sys
    build(draft='--draft' in sys.argv)
