"""Blind source-only annotation. Never reads branch outputs, model calls or labels."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path('runs/passage-coverage-final-2026-10-30')
# Source-only criteria fixed before any new model outputs are inspected.
# Filled from retained original HTML; absent source stays unjudged.
SPECS = {'f01': {'C009': ['jobs := make ( chan int , 5 )',
                  'the more value will be false if jobs has been close d and all values in the channel have '
                  'already been received.',
                  'Reading from a closed channel succeeds immediately, returning the zero value of the '
                  'underlying type.',
                  'The optional second return value is true if the value received was delivered by a '
                  'successful send operation to the channel, or false if it was a zero value generated '
                  'because the channel is closed and empty.'],
         'C001': ['for v := range ch exits cleanly when the channel is closed and drained. After close, the '
                  'channel returns all remaining buffered values first, then returns the zero value with ok '
                  '= false , and range exits.']},
 'f02': {'C006': ['If the mapping is shared and backed by a file (not MAP_ANONYMOUS ) the underlying file '
                  'medium is only guaranteed to be written after it is passed to the msync(2) system call, '
                  'but may also be flushed by the system at any other time.',
                  'In contrast, if the mapping is private (the MAP_PRIVATE flag is set), the changes will '
                  'neither be seen by other processes nor written to the file.'],
         'C016': ['MAP_SHARED Share this mapping. Updates to the mapping are visible to other processes '
                  'mapping the same region, and (in the case of file-backed mappings) are carried through to '
                  'the underlying file.',
                  '(To precisely control when updates are carried through to the underlying file requires '
                  'the use of msync(2) .)',
                  'MAP_PRIVATE Create a private copy-on-write mapping. Updates to the mapping are not '
                  'visible to other processes mapping the same file, and are not carried through to the '
                  'underlying file.']},
 'f03': {'C005': ['Increase the capacity of the vector (the total number of elements that the vector can '
                  "hold without requiring reallocation) to a value that's greater or equal to new_cap .",
                  'If new_cap is greater than the current capacity() , new storage is allocated, otherwise '
                  'the function does nothing.',
                  'reserve() does not change the size of the vector.',
                  'If new_cap is greater than capacity() , all iterators (including the end() iterator) and '
                  'all references to the elements are invalidated. Otherwise, no iterators or references are '
                  'invalidated.']},
 'f04': {'C003': ['If the specified key is not already associated with a value, attempts to compute its '
                  'value using the given mapping function and enters it into this map unless null .',
                  'The entire method invocation is performed atomically. The supplied function is invoked '
                  'exactly once per invocation of this method if the key is absent, else not at all.',
                  'the current (existing or computed) value associated with the specified key, or null if '
                  'the computed value is null'],
         'C005': ['If the specified key is not already associated with a value, attempts to compute its '
                  'value using the given mapping function and enters it into this map unless null .',
                  'The entire method invocation is performed atomically, so the function is applied at most '
                  'once per key.',
                  'the current (existing or computed) value associated with the specified key, or null if '
                  'the computed value is null']},
 'f06': {'C007': ['If a startup probe is configured, Kubernetes does not execute liveness or readiness '
                  'probes until the startup probe succeeds, allowing the application time to finish its '
                  'initialization.',
                  'If the startup probe fails, the kubelet kills the container, and the container is '
                  'subjected to its restart policy .',
                  'For the case of a startup or liveness probe, if at least failureThreshold probes have '
                  'failed, Kubernetes treats the container as unhealthy and triggers a restart for that '
                  'specific container.'],
         'C009': ['Startup Probe - determines if a container’s application has started. While it’s failing, '
                  'liveness and readiness probes are disabled.',
                  'When it fires: from container start, every periodSeconds , until it passes or exceeds '
                  'failureThreshold Failure action: if it never passes within failureThreshold × '
                  'periodSeconds , container is killed',
                  'Startup probe SUCCESS → Startup probe stops → Liveness probe: STARTS (after its own '
                  'initialDelaySeconds) → Readiness probe: STARTS (after its own initialDelaySeconds)']},
 'f08': {'C011': ['The ToList<TSource>(IEnumerable<TSource>) method forces immediate query evaluation and '
                  'returns a List<T> that contains the query results.',
                  'You can append this method to your query in order to obtain a cached copy of the query '
                  'results.']}}
PARTIAL = {'f01': {'C006': 'safechan wrapper documents draining buffered values and timeout zero/false, but not '
                 'standard Go closed buffered receive draining followed by zero/false',
         'C020': 'concrete buffered close example drains queued values then receives zero, but no comma-ok '
                 'false behavior',
         'C003': 'concrete closed buffered channel range example drains 0..4, but channel zero-value receive '
                 'and comma-ok false are absent; map comma-ok examples are a different operation'},
 'f02': {'C008': 'Rust mmap-io wrapper documents private COW no writeback and no-op wrapper flush, but does '
                 'not name MAP_PRIVATE or establish the native Linux msync contract'},
 'f03': {'C015': 'clear/shrink article has concrete reserve capacity and size examples, but no reserve '
                 'reallocation and iterator-invalidation rule',
         'C004': 'reserve capacity-not-size and general push_back reallocation invalidation are explained, '
                 'but reserve request-versus-capacity and no-reallocation preservation conditions are not '
                 'stated'},
 'f04': {'C008': 'general ConcurrentMap interface atomicity explanation includes computeIfAbsent but not '
                 'ConcurrentHashMap function invocation and null-result absent-mapping semantics',
         'C004': 'atomic computation is explained, but the claim that a null mapping-function result throws '
                 'NullPointerException is contradicted by retained C003/C005 API documentation, which states '
                 'null result creates no mapping'},
 'f06': {'C004': 'concrete startupProbe suppression of liveness is explained, but both readiness suppression '
                 'and startup failure/restart action are not established; source reports the project fix '
                 'completed and tested'},
 'f07': {'C004': 'discusses atomic upsert operations and single-operation examples; updateMany is only a '
                 'navigation link and multi-document interleaving is not explained',
         'C014': 'documents updateMany updating all matches and bulk commands; no per-document versus '
                 'whole-operation atomicity or interleaving guarantee',
         'C016': 'explicit updateMany non-atomic and transaction alternative, but does not establish '
                 'concurrent operations interleaving during the multi-update'},
 'f08': {'C006': 'materialization to List is mentioned, but ToList cached result reuse is not; '
                 'query-plan/command caching is a different relation',
         'C012': 'materialization to List is mentioned, but ToList cached result reuse is not; '
                 'query-plan/command caching is a different relation'}}
PARTIAL_SPECS = {'f01': {'C006': ['Drain[T](ch) Non-blocking read of all buffered values',
                  'RecvTimeout[T](ch, d) Receive with timeout; returns zero + false on expiry'],
         'C020': ['wg . Wait ( ) close ( ch ) fmt . Println ( <- ch ) // 10 fmt . Println ( <- ch ) // 11 '
                  'fmt . Println ( <- ch ) // 0',
                  '버퍼에 저장된 값은 고대로 읽어오고 값이 없다면 zero value를 가져온다.'],
         'C003': ['// Channels can be closed and iterated channel := make ( chan int , 5 ) for i := 0 ; i < '
                  '5 ; i ++ { channel <- i } close ( channel ) for value := range channel { fmt . Println ( '
                  'value ) } /* 0 1 2 3 4 */']},
 'f02': {'C008': ['open_cow_writable maps a file privately: write to it freely, the file never changes',
                  'flush() is synchronous ( msync(MS_SYNC) on Unix, FlushViewOfFile + FlushFileBuffers on '
                  'Windows).',
                  'written pages are copied on first write, the changes are visible through this mapping '
                  'only, and they never reach the file. flush() is a no-op']},
 'f03': {'C015': ['data.reserve( 10 ); // capacity를 10으로 확보 data.pop_back(); // 1개 제거 (size = 4, capacity = '
                  '10)'],
         'C004': ['Note that reserve() changes capacity, not size - the vector still has zero elements until '
                  'you push them.',
                  'Because growing moves the storage, any pointer, reference, or iterator you saved into the '
                  'vector becomes dangling after a push_back that reallocates:']},
 'f04': {'C008': ['computeIfAbsent(K, Function) , computeIfPresent(K, BiFunction) , compute(K,BiFunction) , '
                  'and merge(K, V, BiFunction)',
                  'The default implementations in java.util.Map<K, V> do not guarantee atomicity, but in the '
                  'java.util.concurrent.ConcurrentMap<K, V> overriding defaults these use lock free '
                  'techniques to achieve atomicity'],
         'C004': ['It guarantees atomicity when checking for the key, computing the value, and inserting it '
                  'into the map.',
                  'While ConcurrentHashMap does not allow null keys or values, the mapping function might '
                  'inadvertently return a null value. This will result in a NullPointerException .']},
 'f06': {'C004': ['With a startupProbe , the livenessProbe is disabled until the startup probe succeeds, '
                  'preventing premature kills during the flannel initialization window.']},
 'f07': {'C004': ['Upsert operations are atomic . This ensures that the operation is completed in a single '
                  'step without any interference from other operations.'],
         'C014': ['Sends multiple insertOne , updateOne , updateMany , replaceOne , deleteOne , and/or '
                  'deleteMany operations to the MongoDB server in one command.'],
         'C016': ['updateOne() operation updates one document is atomic, if we try to modify multiple '
                  'documents by using a single operation like updateMany() is not atomic because of multiple '
                  'documents are modifying in the collection.',
                  'If we want to make multiple write operations as atomic we need a transaction.']},
 'f08': {'C006': ['Query execution is deferred until the results are needed, such as during a foreach (C#) '
                  'or For Each (Visual Basic) enumeration or when it is assigned to fill a '
                  'xref:System.Collections.Generic.List`1 collection.'],
         'C012': ['Query execution is deferred until the results are needed, such as during a foreach (C#) '
                  'or For Each (Visual Basic) enumeration or when it is assigned to fill a List<T> '
                  'collection.']}}
NEGATIVE = {}
SOURCE_NEGATIVE = {'f05': {'C005': 'full inspected body is an InnoDB monitoring dashboard, with no foreign key or re-enable '
                 'validation semantics',
         'C013': 'full inspected body describes converting InnoDB dumps to MyISAM, not toggling InnoDB '
                 'foreign_key_checks or retrospective validation'},
 'f08': {'C014': 'full inspected body explains Haskell iteratee streaming; no .NET Enumerable.ToList '
                 'materialization or cached query-result relation'}}
SCOPE_WARNINGS = {'f01': {'C005': 'retained repository page exposes file listings and README loading, not the linked '
                 'closing-channel document; source behavior remains unobserved',
         'C011': 'retained HTML canonical text is only Human Verification; article content unobserved',
         'C008': 'retained Go Tour page canonical text is only a 25-character application shell; article '
                 'content unobserved'},
 'f06': {'C018': 'retained response is an explicit 404 page; requested probe article unobserved',
         'C009': 'supports startup gating and failureThreshold leading to container killed; explicit '
                 'respects restartPolicy wording is only in its liveness section, so this source is not '
                 'proof of unconditional restart after startup failure'}}
CRITERIA = {'f01': 'After a buffered Go channel is closed, queued values can still be received; after it is drained a '
        'receive returns the element zero value and comma-ok is false.',
 'f02': 'MAP_PRIVATE is copy-on-write and private modifications are not written back to the underlying file; '
        'msync does not turn a private mapping into shared file-backed writeback. A correct rejection of the '
        'writeback premise is positive.',
 'f03': 'vector::reserve changes capacity without changing size; capacity grows/reallocation happens when '
        'the request exceeds capacity, and reallocation invalidates iterators/references while no '
        'reallocation preserves them.',
 'f04': 'ConcurrentHashMap.computeIfAbsent performs the absent-key computation atomically; a null function '
        'result leaves no mapping, so a later absent-key invocation can compute again. It is not a '
        'forever-once guarantee per key.',
 'f05': 'Re-enabling MySQL/InnoDB foreign_key_checks does not scan or retrospectively validate rows added '
        'while checks were disabled; those pre-existing inconsistent rows are not repaired by re-enabling.',
 'f06': 'A configured startupProbe suppresses both liveness and readiness probes until it succeeds; startup '
        'failure reaching its failure threshold kills/restarts the container according to restart policy.',
 'f07': 'MongoDB updateMany makes individual document updates atomic, but the whole multi-document operation '
        'is not atomic and other operations can interleave unless a transaction supplies the required '
        'isolation.',
 'f08': 'Enumerable.ToList forces immediate evaluation and materializes a List containing a cached copy of '
        'query results; enumerating that list reuses the materialized results instead of re-executing the '
        'original query. It does not automatically memoize future ToList calls.'}

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
    dest = ROOT / 'gold-audit/annotation-draft.json' if draft else Path('feasibility/passage-coverage-final-gold-v1.json')
    assert not Path('feasibility/passage-coverage-final-gold-v1.json').exists(), 'gold already frozen; never relabel after predictions inspected'
    pool = load(ROOT / 'retrieval/corpus.json')
    assert len(pool['cases']) == 8
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
    paths = [dest, Path('feasibility/passage-coverage-final-cases-v1.json'), ROOT / 'retrieval/corpus.json', peer_path]
    (ROOT / 'gold-freeze.json').write_text(json.dumps({str(p): sha(p.read_bytes()) for p in paths}, indent=2) + '\n')
    print('known', sum(g['grade'] is not None for r in rows for g in r['gold']),
          'positive', sum(g['grade'] == 2 for r in rows for g in r['gold']))


if __name__ == '__main__':
    import sys
    build(draft='--draft' in sys.argv)
