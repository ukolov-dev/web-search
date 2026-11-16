"""Offline, source-grounded A/B analysis; stdlib only, no keys or network."""
import collections
import copy
import hashlib
import json
import random
import statistics
from pathlib import Path

ROOT = Path('runs/evidence-layer-2026-10-30')
BASE = json.loads((ROOT / 'baseline-live/results.json').read_text())
LAYER = json.loads((ROOT / 'layer-v2/results.json').read_text())
FIRST = json.loads((ROOT / 'first-round-v2.json').read_text())
SERP = json.loads(Path('feasibility/evidence-layer-serp-gold-v1.json').read_text())
BODY = json.loads(Path('feasibility/evidence-layer-body-gold-v1.json').read_text())
base = {c['id']: c for c in BASE['cases']}
layer = {c['id']: c for c in LAYER['cases']}
first = {c['id']: c for c in FIRST['cases']}
gold_serp = {c['id']: c for c in SERP['cases']}
gold_body = {c['id']: c for c in BODY['cases']}
assert base.keys() == layer.keys() == first.keys() == gold_serp.keys() == gold_body.keys()
for path, expected in json.loads((ROOT / 'gold-freeze-before-metrics.json').read_text()).items():
    assert hashlib.sha256(Path(path).read_bytes()).hexdigest() == expected, 'gold changed'


def direct(labels):
    return {cid for cid, kind in labels.items() if kind == 0}


def metrics(samples):
    tp = fp = fn = tn = top10 = returned_fp = 0
    has_rank = False
    recalls = []
    for judgments, predicted, ranked in samples:
        known = {j['id'] for j in judgments}
        positive = {j['id'] for j in judgments if j['grade'] == 2}
        assert predicted <= known, 'unjudged must not be a false positive'
        t = len(predicted & positive)
        tp += t
        fp += len(predicted - positive)
        fn += len(positive - predicted)
        tn += len(known - positive - predicted)
        if positive:
            recalls.append(t / len(positive))
        has_rank |= bool(ranked)
        top10 += len(set(ranked[:10]) & positive)
        returned_fp += len((set(ranked[:10]) & predicted) - positive)
    return dict(queries=len(samples), tp=tp, fp=fp, fn=fn, tn=tn,
                precision=tp / (tp + fp) if tp + fp else None,
                recall=tp / (tp + fn) if tp + fn else None,
                fpr=fp / (fp + tn) if fp + tn else None,
                macro_recall_positive_queries=statistics.mean(recalls) if recalls else None,
                returned_direct_fp=returned_fp if has_rank else None, known_relevant_top10=top10 if has_rank else None)


def baseline_final(cid):
    labels = dict(layer[cid]['initial_labels']) if base[cid]['classifier_valid'] else {}
    for r in base[cid]['results']:
        if r['classification'] is not None:
            labels[r['id']] = ['direct', 'background', 'irrelevant'].index(r['classification'])
    assert len(direct(labels)) == base[cid]['direct_candidates_predicted'], 'unseen baseline promotion'
    return labels


def samples(ids, stage, branch, strict=False):
    result = []
    for cid in ids:
        judgments = copy.deepcopy((gold_serp if stage == 'SERP' else gold_body)[cid]['judgments'])
        if strict:
            for j in judgments:
                if j.get('borderline'):
                    j['grade'] = 1
        if branch == 'baseline':
            labels = layer[cid]['initial_labels'] if stage == 'SERP' else baseline_final(cid)
            ranked = [r['id'] for r in base[cid]['results']]
        else:
            labels = first[cid]['first_round_labels'] if stage == 'SERP' else layer[cid]['final_labels']
            ranked = layer[cid]['final_top10']
        result.append((judgments, direct(labels), ranked if stage != "SERP" else []))
    return result


def percentile(values, q):
    return sorted(values)[int((len(values) - 1) * q)] if values else None


def bootstrap(ids):
    rng = random.Random(20261030)
    recall, precision, false_direct = [], [], []
    for _ in range(4000):
        selected = rng.choices(ids, k=len(ids))
        a, b = (metrics(samples(selected, 'BODY', branch)) for branch in ('baseline', 'layer'))
        if a['recall'] is not None and b['recall'] is not None:
            recall.append(b['recall'] - a['recall'])
        if a['precision'] is not None and b['precision'] is not None:
            precision.append(b['precision'] - a['precision'])
        false_direct.append(b['fp'] - a['fp'])
    return {k: [percentile(v, .025), percentile(v, .975)] for k, v in
            [('recall_delta', recall), ('precision_delta', precision), ('fp_delta', false_direct)]}


def usage(calls):
    costs = [c.get('response', {}).get('usage', {}).get('cost') for c in calls]
    latencies = [c['elapsed_ms'] for c in calls if 'elapsed_ms' in c]
    return dict(calls=len(calls), known_cost_usd=sum(c for c in costs if c is not None),
                unknown_cost_calls=sum(c is None for c in costs),
                completion_tokens=sum(c.get('response', {}).get('usage', {}).get('completion_tokens', 0) for c in calls),
                reasoning_tokens=sum(c.get('response', {}).get('usage', {}).get('completion_tokens_details', {}).get('reasoning_tokens', 0) for c in calls),
                call_latency_median_ms=statistics.median(latencies) if latencies else None,
                providers=dict(collections.Counter(c.get('response', {}).get('provider', 'unknown') for c in calls)),
                finishes=dict(collections.Counter(c.get('response', {}).get('choices', [{}])[0].get('finish_reason', 'no_response') for c in calls)))


ids = list(base)
valid = [cid for cid in ids if base[cid]['classifier_valid'] and layer[cid]['classifier_valid']]
groups = {}
for name, selected in [('all12_availability_adjusted', ids), ('paired_initial_valid', valid)]:
    groups[name] = {stage: {branch: metrics(samples(selected, stage, branch))
                           for branch in ('baseline', 'layer')} for stage in ('SERP', 'BODY')}
    groups[name]['BODY_strict_sensitivity'] = {b: metrics(samples(selected, 'BODY', b, True))
                                             for b in ('baseline', 'layer')}
    groups[name]['BODY_delta_bootstrap_ci95'] = bootstrap(selected)

per_case = []
proof_count = 0
for cid in ids:
    c = layer[cid]
    assert c['candidates'] == base[cid]['candidates'], 'pool drift'
    assert len(c['initial_calls_shared']) + len(c['new_calls']) <= 3
    assert c['logical_reads'] <= 4
    for call in c['initial_calls_shared']:
        assert call in base[cid]['model_calls'], 'different initial call'
    for call in c['new_calls']:
        assert call['input']['reasoning']['effort'] == 'none'
        assert call['context_chars'] <= 24000
    for did, proofs in c['proofs'].items():
        assert c['final_labels'][did] == 0 and proofs
        cand = next(x for x in c['candidates'] if x['id'] == did)
        path = ROOT / 'layer-v2' / cid / f'{cid}-{did}-document.json'
        doc = json.loads(path.read_text()) if path.exists() else None
        for p in proofs:
            field = p['field']
            text = cand[field] if field in ('title', 'snippet') else (
                doc['text'] if field == 'body' else (doc.get('page_title') or doc.get('title') or '')[:500])
            text = ' '.join(text.split())
            assert text.encode()[p['start']:p['end']].decode() == p['quote'], 'invented source range'
            assert hashlib.sha256(text.encode()).hexdigest() == p['normalized_text_hash']
            if field == 'body':
                assert doc['text_hash'] == p['text_hash']
            proof_count += 1
    assert direct(c['final_labels']) == set(c['proofs']), 'direct bypass'
    per_case.append(dict(id=cid, query=c['query'], initial_valid=c['classifier_valid'],
                         first_evidence_valid=first[cid]['first_evidence_valid'],
                         baseline=metrics(samples([cid], 'BODY', 'baseline')),
                         layer=metrics(samples([cid], 'BODY', 'layer')),
                         errors=c['errors'], rejected=c['rejected'],
                         shared_initial_ms=c['logical_shared_initial_ms'],
                         baseline_model_ms=sum(x.get('elapsed_ms', 0) for x in base[cid]['model_calls']),
                         layer_logical_model_ms=c['logical_shared_initial_ms'] + sum(x.get('elapsed_ms', 0) for x in c['new_calls']),
                         layer_physical_ms=c['new_model_elapsed_ms'],
                         cached_reads=c['cached_reads'], network_reads=c['network_reads']))

physical = {'baseline': usage([x for c in base.values() for x in c['model_calls']])}
for branch in ('layer-live', 'layer-final', 'layer-production', 'layer-v2'):
    r = json.loads((ROOT / branch / 'results.json').read_text())
    physical[branch] = usage([x for c in r['cases'] for x in c['new_calls']])
    assert abs(physical[branch]['known_cost_usd'] - r['new_reported_cost_usd']) < 1e-10
shared_usage = usage([x for c in layer.values() for x in c['initial_calls_shared']])
summary = dict(queries=len(ids), candidates=sum(len(c['candidates']) for c in base.values()),
               serp_positive=sum(j['grade'] == 2 for c in SERP['cases'] for j in c['judgments']),
               body_positive=sum(j['grade'] == 2 for c in BODY['cases'] for j in c['judgments']),
               paired_initial_valid_ids=valid, invalid_initial_ids=[cid for cid in ids if cid not in valid],
               groups=groups, per_case=per_case, physical_usage=physical,
               logical_baseline_cost_usd=physical['baseline']['known_cost_usd'],
               logical_layer_cost_usd=shared_usage['known_cost_usd'] + physical['layer-v2']['known_cost_usd'],
               total_known_experiment_cost_usd=sum(v['known_cost_usd'] for v in physical.values()),
               shared_initial_usage=shared_usage,
               predictions={cid:dict(initial=layer[cid]['initial_labels'],baseline_final=baseline_final(cid),first_round=first[cid]['first_round_labels'],layer_final=layer[cid]['final_labels'],baseline_top10=[x['id'] for x in base[cid]['results']],layer_top10=layer[cid]['final_top10'],proofs=layer[cid]['proofs'],evidence_memory=layer[cid]['memory']) for cid in ids},
               verification=dict(passed=True, paired_pools=True, identical_shared_initial_calls=True,
                                 source_range_proofs=proof_count, no_unverified_direct=True,
                                 max3_calls_max4_reads=True, max24000_context_chars=True,
                                 serp_gold_frozen_before_layer=True, body_gold_frozen_before_metrics=True),
               notes=['New12 queries, single author judgments, not independent acceptance.',
                      'SERP gold fixed before layer inference; body-aware gold before prediction-quality inspection.',
                      'Repeated technical high/low trials and v1 regression retained; v2 fixes non-support citation handling, reviewer none, initial classifier unchanged.',
                      'v2 is development verification after v1 error analysis, not a fresh unseen holdout.',
                      'Body snapshots selected mainly by old baseline; same cached bytes on shared reads.',
                      'No full-web recall claim. Availability-adjusted invalid initial cases produce no direct.',
                      'Reported cost excludes unreported cost of separate release MCP smoke.'])
# Small arithmetic controls; unrelated/unjudged is never silently treated as direct-negative.
control = metrics([([dict(id='a', grade=2), dict(id='b', grade=1)], {'a', 'b'}, ['a', 'b'])])
assert (control['tp'], control['fp'], control['fn'], control['tn']) == (1, 1, 0, 0)
assert metrics([([dict(id='a', grade=2)], set(), [])])['recall'] == 0
Path('feasibility/evidence-layer-results-2026-10-30.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
print(json.dumps({k: v for k, v in summary.items() if k in ('queries', 'candidates', 'groups', 'physical_usage', 'total_known_experiment_cost_usd', 'verification')}, ensure_ascii=False, indent=2))
