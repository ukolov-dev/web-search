"""Offline paired analysis. Run only after source gold is frozen; never tunes runtime."""
import collections
import hashlib
import json
import random
import runpy
import statistics
import sys
from pathlib import Path

ADMISSION_LITERALS = '--admission-literals' in sys.argv[1:]
DIRECT_COVERAGE = '--direct-coverage' in sys.argv[1:]
CONFIRM_COVERAGE = '--passage-coverage-confirm' in sys.argv[1:]
FINAL_COVERAGE = '--passage-coverage-final' in sys.argv[1:]
PASSAGE_COVERAGE = '--passage-coverage' in sys.argv[1:]
PROFILE = 'admission-literals' if ADMISSION_LITERALS else 'passage-coverage-confirm' if CONFIRM_COVERAGE else 'passage-coverage-final' if FINAL_COVERAGE else 'passage-coverage' if PASSAGE_COVERAGE else ('direct-coverage' if DIRECT_COVERAGE else 'extraction-selection')
ROOT = Path(f'runs/{PROFILE}-2026-10-30')
CORPUS = Path(f'feasibility/{PROFILE}-corpus-2026-10-30.json')
RESULTS = Path(f'feasibility/{PROFILE}-results-2026-10-30.json')
ORIGINAL20 = 'b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6'
BRANCHES = ('baseline', 'fixed')


def load(path):
    return json.loads(Path(path).read_text())


def dump(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def compact(text):
    return ' '.join(text.split())


def metrics(rows):
    tp = fp = fn = unknown = 0
    for gold, predicted in rows:
        positive = {ident for ident, g in gold.items() if g['grade'] == 2}
        known = {ident for ident, g in gold.items() if g['grade'] is not None}
        tp += len(positive & predicted)
        fp += len((predicted & known) - positive)
        fn += len(positive - predicted)
        unknown += len(predicted - known)
    return dict(tp=tp, fp=fp, fn=fn, unjudged_direct=unknown,
                recall=tp / (tp + fn) if tp + fn else None,
                precision_judged=tp / (tp + fp) if tp + fp else None)


def median(values):
    return statistics.median(values) if values else None


def combine():
    for name in ('gold-freeze.json', 'code-freeze.json'):
        for path, digest in load(ROOT / name).items():
            assert sha(Path(path).read_bytes()) == digest, ('changed frozen input', path)
    gold = load(f'feasibility/{PROFILE}-gold-v1.json')
    data = dict(pool=load(ROOT / 'retrieval/corpus.json'), gold=gold,
                branches={b: load(ROOT / b / 'results.json') for b in BRANCHES},
                manifests={b: load(ROOT / b / 'manifest.json') for b in BRANCHES},
                raw_snapshots={p.name: load(p) for p in (ROOT / 'raw-cache').glob('*.json') if p.name != 'blocks.json'},
                documents={b: {p.name: load(p) for p in (ROOT / b).glob('*-document.json')} for b in BRANCHES},
                gold_html={}, gold_freeze=load(ROOT / 'gold-freeze.json'), code_freeze=load(ROOT / 'code-freeze.json'))
    for row in gold['cases']:
        for g in row['gold']:
            path = g.get('snapshot')
            if path and Path(path).suffix == '.html':
                data['gold_html'][path] = Path(path).read_text()
    dump(CORPUS, data)
    return data


def verify_gold(data):
    source_text = runpy.run_path(f'scripts/{PROFILE}-gold-2026-10-30.py')['source_text']
    for row in data['gold']['cases']:
        for g in row['gold']:
            assert g['grade'] in (None, 0, 1, 2)
            if not g.get('support'):
                assert g['grade'] != 2, ('positive needs source witness', row['id'], g['id'])
                continue
            assert g['field'] == 'raw_html_text'
            snapshot = g['snapshot']
            if Path(snapshot).suffix == '.json':
                saved = data['raw_snapshots'][Path(snapshot).name]
                assert saved['url'] == g['url']
                html = saved['fetched']['html']
                assert saved['html_hash'] == sha(html.encode())
            else:
                html = data['gold_html'][snapshot]
            text = source_text(html)
            assert sha(html.encode()) == g['html_hash']
            assert sha(text.encode()) == g['normalized_text_hash']
            for support in g['support']:
                assert text.encode()[support['start']:support['end']].decode() == support['quote'], 'gold UTF8 source range'


def verify_proofs(row, branch_documents):
    candidates = {c['id']: c for c in row['candidates']}
    records = {r['id']: r for r in row['page_verification']['documents']}
    memory = row['page_verification']['evidence_memory']
    entries = {e['id']: e for e in memory.get('entries', [])} if isinstance(memory, dict) else {}
    count = 0
    for result in row['results']:
        if result['classification'] != 'direct':
            continue
        entry = entries[result['id']]
        assert entry['audit_supported'] is True, 'every final direct needs accepted audit'
        assert all(c['status'] == 'supported' for c in entry['checks'][:len(memory['requirements'])])
        proofs = result['requirement_evidence']
        assert proofs and any(p.get('requirement') == 'R0' for p in proofs)
        candidate = candidates[result['id']]
        for proof in proofs:
            field = proof['field']
            if field in ('title', 'snippet'):
                text = compact(candidate[field])
                text_hash = sha(text.encode())
                assert proof['source_url'] == candidate['url']
            else:
                doc = branch_documents[f"{row['id']}-{result['id']}-document.json"]
                assert sha(doc['text'].encode()) == doc['text_hash']
                assert records[result['id']]['text_hash'] == doc['text_hash']
                assert proof['source_url'] == records[result['id']]['final_url']
                if field == 'body':
                    text, text_hash = compact(doc['text']), doc['text_hash']
                else:
                    assert field == 'page_title'
                    title = doc.get('page_title', '')
                    title = title if title.strip() else doc.get('title', '')
                    text = compact(title[:500])
                    text_hash = sha(text.encode())
            assert text.encode()[proof['start']:proof['end']].decode() == proof['quote'], 'branch-specific UTF8 proof range'
            assert proof['text_hash'] == text_hash and proof['normalized_text_hash'] == sha(text.encode())
            if not proof.get('context_only'):
                assert 16 <= len(proof['quote']) <= 240
            count += 1
    return count


def verify_conflicts(row, branch_documents):
    memory = row['page_verification']['evidence_memory']
    conflicts = memory.get('audit_conflicts', []) if isinstance(memory, dict) else []
    assert len(conflicts) <= 4
    entries = {e['id']: e for e in memory.get('entries', [])} if isinstance(memory, dict) else {}
    candidates = {c['id']: c for c in row['candidates']}
    records = {r['id']: r for r in row['page_verification']['documents']}
    predicted = {r['id'] for r in row['results'] if r['classification'] == 'direct'}
    seen = set()
    for conflict in conflicts:
        pair = tuple(sorted((conflict['left'], conflict['right'])))
        assert pair[0] != pair[1] and pair not in seen, 'unique distinct conflict pair'
        seen.add(pair)
        assert not predicted.intersection(pair), 'quarantined conflict cannot leave either source direct'
        for side in ('left', 'right'):
            ident = conflict[side]
            entry = entries[ident]
            assert entry['audit_supported'] is False and 'unresolved_source_conflict' in entry['audit_reason']
            passage = next(p for p in entry['passages'] if p['id'] == conflict[f'{side}_passage'])
            quote = conflict[f'{side}_quote']
            assert passage['field'] in ('snippet', 'body') and 16 <= len(quote) <= 240
            if passage['field'] == 'snippet':
                text = compact(candidates[ident]['snippet'])
                text_hash = sha(text.encode())
                source_url = candidates[ident]['url']
            else:
                doc = branch_documents[f"{row['id']}-{ident}-document.json"]
                assert sha(doc['text'].encode()) == doc['text_hash']
                assert records[ident]['text_hash'] == doc['text_hash']
                text, text_hash = compact(doc['text']), doc['text_hash']
                source_url = records[ident]['final_url']
            assert passage['text_hash'] == text_hash and passage['normalized_text_hash'] == sha(text.encode())
            assert passage['source_url'] == source_url
            supplied = text.encode()[passage['start']:passage['end']].decode()
            assert quote in supplied, 'conflict quote must exist inside its supplied branch-specific source passage'
    return len(conflicts)


def analyze(data):
    gold = {r['id']: {g['id']: g for g in r['gold']} for r in data['gold']['cases']}
    pool = {r['case']['id']: r for r in data['pool']['cases']}
    rows = {b: {r['id']: r for r in data['branches'][b]['cases']} for b in BRANCHES}
    assert gold.keys() == pool.keys() == rows['baseline'].keys() == rows['fixed'].keys()
    versions = ('request-evidence-v9-reference-context', 'request-evidence-v10-admission-literals') if ADMISSION_LITERALS else ('request-evidence-v6-context-audit', 'request-evidence-v9-reference-context') if CONFIRM_COVERAGE else ('request-evidence-v6-context-audit', 'request-evidence-v8-atomic-contract') if FINAL_COVERAGE else ('request-evidence-v6-context-audit', 'request-evidence-v7-diverse-coverage') if PASSAGE_COVERAGE else (('request-evidence-v5-reference-read', 'request-evidence-v6-context-audit') if DIRECT_COVERAGE else ('request-evidence-v4-audited', 'request-evidence-v5-reference-read'))
    for branch, evidence_version in zip(BRANCHES, versions):
        manifest = data['manifests'][branch]
        assert manifest['evidence_version'] == evidence_version
        assert manifest['classifier'] == 'conservative-evidence-v4-bounded-output'
        assert manifest['initial_classifier_bypass'] is False and manifest['raw_acquisition_cache'] is True
        assert manifest['reranker'] == 'Qwen3-Reranker-0.6B-4bit' and manifest['experiment_profile'] == PROFILE
    assert data['manifests']['baseline']['binary_sha256'] != data['manifests']['fixed']['binary_sha256']
    output = dict(protocol=dict(cases=len(pool), cards=sum(len(p['candidates']) for p in pool.values()),
                               known_gold=sum(g['grade'] is not None for gs in gold.values() for g in gs.values()),
                               positive_gold=sum(g['grade'] == 2 for gs in gold.values() for g in gs.values()),
                               unjudged_gold=sum(g['grade'] is None for gs in gold.values() for g in gs.values()),
                               gold=data['gold']['protocol'], comparison='full initial+coverage+audit+localQwen; identical frozen live SERP and raw acquisition; branch-specific extraction and document selection',
                               timing='sequential replay; raw/snapshot and provider cache differ; not isolated SLA',
                               read_metrics='selected includes host-disabled skips; attempted excludes those skips; success requires actual extracted document; ratios use known positive retrieved sources'),
                  branches={}, cases=[])
    samples = {}
    for branch in BRANCHES:
        for doc in data['documents'][branch].values():
            assert sha(doc['text'].encode()) == doc['text_hash'], 'branch extracted-document hash'
        samples[branch] = []
        returned_samples, calls, initials, reranks, elapsed = [], [], [], [], []
        errors = collections.Counter()
        proof_count = conflict_count = initial_valid = selected_positive = attempted_positive = extracted_positive = pool_positive = cache_hits = 0
        positive_total = reads_total = 0
        for cid, row in rows[branch].items():
            assert row['candidates'] == pool[cid]['candidates'] and row['sources'] == pool[cid]['sources']
            budget = row['budget']
            assert len(row['model_calls']) <= 3 and budget['document_reads'] <= 4 and budget['reranker_calls'] <= 4
            assert budget['classification_calls'] == len(row['model_calls'])
            initial = [c for c in row['model_calls'] if 'conservative retrieval reviewer' in c.get('input', {}).get('messages', [{}])[0].get('content', '')]
            assert len(initial) == 1, 'actual initial classifier call must be present, no bypass'
            initials.extend(initial)
            calls.extend(row['model_calls'])
            initial_valid += row['classifier_valid']
            elapsed.append(row['elapsed_ms'])
            positive = {ident for ident, g in gold[cid].items() if g['grade'] == 2}
            positive_total += len(positive)
            predicted = {r['id'] for r in row['results'] if r['classification'] == 'direct'}
            returned = {r['id'] for r in row['results']}
            samples[branch].append((gold[cid], predicted))
            returned_samples.append((gold[cid], returned))
            proof_count += verify_proofs(row, data['documents'][branch])
            verified_conflicts = verify_conflicts(row, data['documents'][branch])
            conflict_count += verified_conflicts
            documents = row['page_verification']['documents']
            candidates = {c['id']: c for c in row['candidates']}
            for record in documents:
                if 'html_hash' in record:
                    candidate = candidates[record['id']]
                    saved = data['raw_snapshots'][sha(candidate['url'].encode()) + '.json']
                    assert saved['url'] == candidate['url'] and saved['html_hash'] == record['html_hash']
                    assert sha(saved['fetched']['html'].encode()) == saved['html_hash']
                    assert saved['fetched']['url'] == record['final_url'], 'same raw acquisition for branch extraction'
            selected = {d['id'] for d in documents}
            attempted = {d['id'] for d in documents if not d.get('error', '').startswith('host_disabled:')}
            successful = {d['id'] for d in documents if isinstance(d.get('text_hash'), str) and f"{cid}-{d['id']}-document.json" in data['documents'][branch]}
            memory = row['page_verification']['evidence_memory']
            entries = {e['id'] for e in memory.get('entries', [])} if isinstance(memory, dict) else set()
            selected_positive += len(positive & selected)
            attempted_positive += len(positive & attempted)
            extracted_positive += len(positive & successful)
            pool_positive += len(positive & entries)
            reads_total += budget['document_reads']
            cache_hits += sum(d.get('raw_snapshot_cache_hit', False) for d in documents)
            reranks.extend(row['page_verification']['passage_reranker']['calls'])
            errors.update(e.split(':')[0] for e in row['errors'])
            case = next((c for c in output['cases'] if c['id'] == cid), None)
            if case is None:
                case = dict(id=cid, query=row['query'], known_positive=len(positive), branches={})
                output['cases'].append(case)
            case['branches'][branch] = dict(**metrics([(gold[cid], predicted)]), direct=sorted(predicted),
                                            relevant_returned=len(positive & returned), positive_in_coverage_pool=sorted(positive & entries),
                                            positive_read_selected=sorted(positive & selected), positive_attempted=sorted(positive & attempted),
                                            positive_extracted=sorted(positive & successful), initial_valid=row['classifier_valid'],
                                            reads=budget['document_reads'], audit_conflicts_validated=verified_conflicts, errors=row['errors'], status=row['status'])
        stats = metrics(samples[branch])
        costed = [c.get('response', {}).get('usage', {}).get('cost') for c in calls]
        stats.update(initial_valid=initial_valid, initial_attempts=len(initials), initial_length=sum(c['response']['choices'][0]['finish_reason'] == 'length' for c in initials),
                     initial_stop_empty=sum(c['response']['choices'][0]['finish_reason'] == 'stop' and not c['response']['choices'][0]['message'].get('content') for c in initials),
                     initial_reasoning_tokens=sum(c['response']['usage'].get('completion_tokens_details', {}).get('reasoning_tokens', 0) or 0 for c in initials),
                     initial_elapsed_ms_median=median([c['elapsed_ms'] for c in initials]), elapsed_ms_median=median(elapsed), elapsed_ms_sum=sum(elapsed),
                     generative_calls=len(calls), known_cost_usd=sum(v for v in costed if isinstance(v, (int, float))), unpriced_generations=sum(not isinstance(v, (int, float)) for v in costed),
                     prompt_tokens=sum(c.get('response', {}).get('usage', {}).get('prompt_tokens', 0) or 0 for c in calls),
                     completion_tokens=sum(c.get('response', {}).get('usage', {}).get('completion_tokens', 0) or 0 for c in calls),
                     providers=dict(collections.Counter(c.get('response', {}).get('provider', 'unknown') for c in calls)),
                     reads=reads_total, raw_snapshot_cache_hits=cache_hits, known_positive=positive_total,
                     positive_in_coverage_pool=pool_positive, positive_read_selected=selected_positive, positive_attempted=attempted_positive, positive_extracted=extracted_positive,
                     read_recall=attempted_positive / positive_total if positive_total else None, extracted_source_recall=extracted_positive / positive_total if positive_total else None,
                     reranker_calls=len(reranks), reranker_pairs=sum(len(c.get('trace', {}).get('pool_ranges', [])) for c in reranks), reranker_fallbacks=sum(c['status'] != 'ok' for c in reranks),
                     proof_ranges_validated=proof_count, audit_conflicts_validated=conflict_count, errors=dict(errors), all_classes_recall_at10=metrics(returned_samples)['recall'], relevant_returned_at10=metrics(returned_samples)['tp'])
        output['branches'][branch] = stats
    for cid in pool:
        before = rows['baseline'][cid]['model_calls'][0]['input']
        after = rows['fixed'][cid]['model_calls'][0]['input']
        assert before['messages'] == after['messages'] and before['response_format'] == after['response_format']
        assert before['max_tokens'] == after['max_tokens'] and before.get('reasoning') == after.get('reasoning')
        for branch in BRANCHES:
            for call in rows[branch][cid]['model_calls']:
                if 'input' in call:
                    assert 'gold' not in json.loads(call['input']['messages'][1]['content']), 'gold leaked into model input'
    false_ids = {b: {(cid, ident) for cid, row in rows[b].items()
                     for ident in [r['id'] for r in row['results'] if r['classification'] == 'direct']
                     if gold[cid][ident]['grade'] in (0, 1)} for b in BRANCHES}
    output['false_direct_changes'] = dict(new=sorted(false_ids['fixed'] - false_ids['baseline']),
                                          retained=sorted(false_ids['fixed'] & false_ids['baseline']),
                                          removed=sorted(false_ids['baseline'] - false_ids['fixed']))
    relevance_gold = {cid: {ident: dict(g, grade=g.get('source_relevance_grade', g['grade']))
                            for ident, g in gs.items()} for cid, gs in gold.items()}
    output['source_relevance_sensitivity'] = {b: metrics([(relevance_gold[cid], {r['id'] for r in row['results'] if r['classification'] == 'direct'})
                                                         for cid, row in rows[b].items()]) for b in BRANCHES}
    rng, differences = random.Random(20261030), []
    for _ in range(4000):
        chosen = [rng.randrange(len(pool)) for _ in pool]
        values = [metrics([samples[b][i] for i in chosen])['recall'] for b in BRANCHES]
        if None not in values:
            differences.append(values[1] - values[0])
    differences.sort()
    output['recall_delta_query_bootstrap95'] = [differences[int(.025 * len(differences))], differences[min(len(differences) - 1, int(.975 * len(differences)))]] if differences else None
    output['corpus_sha256'] = sha(CORPUS.read_bytes())
    output['original20_sha256'] = sha(Path('feasibility/search-cases.json').read_bytes())
    assert output['original20_sha256'] == ORIGINAL20
    dump(RESULTS, output)
    return output


if __name__ == '__main__':
    assert metrics([({'p': {'grade': 2}, 'n': {'grade': 1}, 'u': {'grade': None}}, {'p', 'n', 'u'})]) == dict(tp=1, fp=1, fn=0, unjudged_direct=1, recall=1.0, precision_judged=.5)
    data = load(CORPUS) if CORPUS.exists() else combine()
    verify_gold(data)
    result = analyze(data)
    print(json.dumps(result['branches'], ensure_ascii=False, indent=2))
