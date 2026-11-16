#!/usr/bin/env python3
"""Offline accounting for the fixed real-Harness cache trial; semantic labels stay manual."""
import argparse
import hashlib
import json
import math
import re
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/harness-cache-2026-11-05'


def load(path):
    return json.loads(path.read_text())


def canonical(url):
    value = urlsplit(url)
    return urlunsplit((value.scheme, value.netloc.lower(), value.path.rstrip('/') or '/', value.query, ''))


def usage(calls):
    known = [v['cost'] for v in calls if isinstance(v.get('cost'), (int, float))]
    return {'calls': len(calls), 'known_cost_usd': sum(known), 'unknown_cost_calls': len(calls) - len(known),
        'input_tokens': sum(v.get('prompt_tokens', 0) for v in calls),
        'output_tokens': sum(v.get('completion_tokens', 0) for v in calls),
        'provider_cached_input_tokens': sum(v.get('prompt_tokens_details', {}).get('cached_tokens', 0) for v in calls)}


def percentile(values, p):
    values = sorted(values)
    return values[max(0, math.ceil(p * len(values)) - 1)] if values else None


def analyze_trial(directory):
    directory=directory.resolve()
    result = load(directory / 'result.json')
    records = []
    for path in sorted((directory / 'mcp-capture').rglob('*-result.json')):
        value = load(path)
        if 'model_calls' in value:
            records.append((path, value))
    outer = [load(p).get('usage') or {} for p in sorted(directory.glob('agent-llm-*.json'))]
    inner = [c.get('response', {}).get('usage', {}) for _, v in records for c in v['model_calls']]
    answers = [b['text'] for m in result.get('assistant_messages', []) for b in m.get('message', {}).get('content', []) if b.get('type') == 'text']
    final = answers[-1] if answers else ''
    tools = []
    spills = {}
    readable = set()
    for line in (directory / 'events.jsonl').read_text().splitlines():
        event = json.loads(line).get('params', {}).get('event', {})
        if event.get('type') != 'tool/result':
            continue
        message = event.get('data', {}).get('message', {})
        for block in message.get('content', []):
            if block.get('type') == 'text':
                text=block.get('text','')
                omitted=re.search(r'\(Omitted (\d+) bytes\. Full formatted result stored at:',text)
                if omitted:spills[message.get('toolCallId',str(len(spills)))]=int(omitted.group(1))
                try:
                    v = json.loads(text)
                    if isinstance(v, dict): tools.append(v)
                except json.JSONDecodeError:
                    pass
    ranked_caches=[load(p) for p in sorted((directory/'mcp-capture').rglob('cache.json'))]
    extra_caches=[]
    for v in tools:
        for doc in v.get('documents', []):
            if doc.get('status') == 'ok':
                readable.update(canonical(u) for u in [doc.get('url'), doc.get('final_url'), *doc.get('hops', [])] if u)
    # Presentation can spill/truncate JSON. Use original capture files for accounting and URL reads.
    for path in sorted((directory/'mcp-capture').rglob('*.json')):
        if path.parent.name.endswith(('-extract','-search','-discover')):
            value=load(path)
            if isinstance(value.get('cache'),dict):extra_caches.append(value['cache'])
            if value.get('text_hash') and value.get('text'):
                readable.update(canonical(u) for u in [value.get('url'),value.get('final_url'),*value.get('hops',[])] if u)
    caches=ranked_caches+extra_caches
    ranked = []
    bodies = []
    for path, value in records:
        pv = value.get('page_verification', {})
        docs = [*pv.get('documents', []), *(value.get('news_navigation') or {}).get('documents', [])]
        for doc in docs:
            if doc.get('text_hash'):
                readable.update(canonical(u) for u in [doc.get('url'), doc.get('final_url'), *doc.get('hops', [])] if u)
        ranked.append({'path': str(path.relative_to(ROOT)), 'results': value['results'], 'documents': docs,
            'classifier_valid': value.get('classifier_valid'), 'errors': value.get('errors'), 'elapsed_ms': value['elapsed_ms'],
            'qwen_calls': len(pv.get('passage_reranker', {}).get('calls', [])),
            'source_hashes': {s['engine'] + ':' + s['query']: s.get('html_hash') for s in value.get('sources', [])}})
        for p in sorted(path.parent.glob('*-document.json')):
            d = load(p)
            bodies.append({'path': str(p.relative_to(ROOT)), 'text_hash': d.get('text_hash'), 'chars': len(d.get('text', ''))})
    urls = list(dict.fromkeys(u.rstrip('.,;:') for u in re.findall(r'https?://[^\s<>\])`]+', final)))
    return {'id': result['id'], 'phase': result['phase'], 'status': result['status'], 'error': result.get('error'),
        'harness': result.get('harness'), 'startup_ms': result.get('startup_ms'), 'task_elapsed_ms': result.get('task_elapsed_ms'),
        'process_elapsed_ms': result['process_elapsed_ms'], 'outer': usage(outer), 'inner': usage(inner),
        'tool_calls': result.get('tool_calls', []), 'answer': final, 'answer_links': urls,
        'links_with_successful_body_read': [u for u in urls if canonical(u) in readable],
        'cache': {key: sum(c.get(key, 0) for c in caches) for key in ['hits', 'misses', 'errors', 'writes', 'http_reads', 'qwen_calls', 'extractions']},
        'ranked_cache': {key: sum(c.get(key,0) for c in ranked_caches) for key in ['hits','misses','errors','writes','http_reads','qwen_calls','extractions']},
        'accounting_scope':'ranked counters complete; extra counters include successful captured probes only, lower bound on whole episode HTTP/extraction work',
        'spilled_tool_results':len(spills),'omitted_presentation_bytes':sum(spills.values()),
        'cache_observed': bool(caches), 'ranked': ranked, 'bodies': bodies}


def aggregate(trials):
    completed = [v for v in trials if v['status'] == 'completed']
    return {'episodes': len(trials), 'completed': sum(v['status'] == 'completed' for v in trials),
        'latency_ms_p50': percentile([v['task_elapsed_ms'] for v in completed], .5),
        'latency_ms_p95': percentile([v['task_elapsed_ms'] for v in completed], .95),
        'total_task_ms': sum(v.get('task_elapsed_ms') or 0 for v in trials),
        'outer_cost_usd': sum(v['outer']['known_cost_usd'] for v in trials),
        'inner_cost_usd': sum(v['inner']['known_cost_usd'] for v in trials),
        'unknown_cost_calls': sum(v[layer]['unknown_cost_calls'] for v in trials for layer in ['outer', 'inner']),
        'outer_calls': sum(v['outer']['calls'] for v in trials), 'inner_calls': sum(v['inner']['calls'] for v in trials),
        'provider_cached_input_tokens': sum(v[layer]['provider_cached_input_tokens'] for v in trials for layer in ['outer', 'inner']),
        'qwen_traced_calls': sum(r['qwen_calls'] for v in trials for r in v['ranked']),
        'spilled_tool_results':sum(v['spilled_tool_results'] for v in trials),
        'ranked_cache':{k:sum(v['ranked_cache'][k] for v in trials) for k in ['hits','misses','errors','writes','http_reads','qwen_calls','extractions']},
        'cache': {k: sum(v['cache'][k] for v in trials) for k in ['hits', 'misses', 'errors', 'writes', 'http_reads', 'qwen_calls', 'extractions']}}


def quality(trials, labels, paired_ids):
    outputs={}
    for phase in ['baseline','cold','warm','recovery-cold','recovery-warm']:
        rows=[v for v in trials if v['phase']==phase]
        ratings=labels['answers'].get(phase,{})
        direct=[];covered=set();counts={k:0 for k in ['C','U','W','M']};paired={k:0 for k in counts};errors=[]
        for row in rows:
            label=ratings.get(row['id'])
            if label is None:raise ValueError('Unreviewed final answer '+phase+'/'+row['id'])
            for fact in label['facts']:
                counts[fact]+=1
                if row['id'] in paired_ids:paired[fact]+=1
            errors.extend(row['id']+'/'+e for e in label['errors'])
            for ranked in row['ranked']:
                for result in ranked['results']:
                    if result['classification']!='direct':continue
                    known=any(d.get('id')==result['id'] and d.get('text_hash') for d in ranked['documents'])
                    verdict=labels['direct_verdicts'].get(row['id'],{}).get(result['url'])
                    if verdict is None:raise ValueError('Unreviewed direct '+row['id']+'/'+result['url'])
                    verdict=verdict if known else 'unknown'
                    assert verdict in ['positive','negative','unknown']
                    direct.append({'id':row['id'],'candidate':result['id'],'url':result['url'],'verdict':verdict,'body_captured':known})
                    if verdict=='positive' and row['id']>'H04':covered.add(row['id'])
        outputs[phase]={'facts':counts,'paired_facts':paired,'technical_gold_denominator':48 if not phase.startswith('recovery') else 6,
            'direct':direct,'direct_counts':{k:sum(v['verdict']==k for v in direct) for k in ['positive','negative','unknown']},
            'direct_task_ids':sorted(covered),'direct_technical_task_coverage':len(covered),
            'important_extra_or_core_error_ids':errors,
            'news_tasks_completed':sum(ratings[v['id']].get('news_complete',False) for v in rows),
            'news_safe_abstentions':sum(ratings[v['id']].get('safe_abstention',False) for v in rows),
            'answer_links':sum(len(v['answer_links']) for v in rows),
            'links_with_successful_body_read':sum(len(v['links_with_successful_body_read']) for v in rows)}
    baseline_fp={v['id']+'/'+v['url'] for v in outputs['baseline']['direct'] if v['verdict']=='negative'}
    for phase in ['cold','warm']:
        outputs[phase]['new_false_direct_ids']=sorted({v['id']+'/'+v['url'] for v in outputs[phase]['direct'] if v['verdict']=='negative'}-baseline_fp)
    return outputs


def source_equivalence(trials):
    by={(v['phase'],v['id']):v for v in trials};pairs=[]
    for case_id in sorted({v['id'] for v in trials if v['phase']=='cold'}):
        cold=by.get(('cold',case_id));warm=by.get(('warm',case_id))
        if not cold or not warm or not cold['ranked'] or not warm['ranked']:continue
        a,b=cold['ranked'][0],warm['ranked'][0]
        def raw(row):return load(ROOT/row['path'])
        ca,wa=raw(a),raw(b)
        ic=[v for v in ca['model_calls'] if v.get('input')];iw=[v for v in wa['model_calls'] if v.get('input')]
        same_input=bool(ic and iw and ic[0]['input']==iw[0]['input'])
        da={canonical(d['url']):d['text_hash'] for d in a['documents'] if d.get('text_hash')}
        db={canonical(d['url']):d['text_hash'] for d in b['documents'] if d.get('text_hash')}
        shared=da.keys()&db.keys()
        def responses(value):
            return [v.get('trace',{}).get('call',{}) for v in value.get('page_verification',{}).get('passage_reranker',{}).get('calls',[])]
        old={v.get('response',{}).get('id'):v.get('response') for v in responses(ca)}
        cached=[v for v in responses(wa) if v.get('cache_hit')]
        pairs.append({'id':case_id,'serp_hashes_equal':a['source_hashes']==b['source_hashes'],
            'first_inner_input_equal':same_input,
            'first_inner_content_equal':bool(same_input and ic[0]['response']['choices'][0]['message'].get('content')==iw[0]['response']['choices'][0]['message'].get('content')),
            'shared_read_documents':len(shared),'shared_document_text_hash_mismatches':sum(da[k]!=db[k] for k in shared),
            'warm_cached_qwen_responses':len(cached),'cached_qwen_not_equal_to_cold':sum(v.get('response')!=old.get(v.get('response',{}).get('id')) for v in cached)})
    return pairs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-root', type=Path, default=RUN)
    parser.add_argument('--quality-file', type=Path, default=ROOT/'feasibility/harness-cache-quality-2026-11-05.json')
    parser.add_argument('--output', type=Path, default=RUN / 'accounting.json')
    args = parser.parse_args()
    phases=['baseline', 'cold', 'warm', 'recovery-cold', 'recovery-warm']
    trials = [analyze_trial(p.parent) for phase in phases for p in sorted((args.run_root / phase).glob('*/result.json'))]
    complete_sets=[{v['id'] for v in trials if v['phase']==phase and v['status']=='completed'} for phase in phases[:3]]
    paired_ids=sorted(set.intersection(*complete_sets))
    out = {'schema': 'harness-cache-accounting-v1', 'source': 'actual native Harness/MCP traces, not synthetic calls',
        'cost': 'provider-reported known cost only; unknown counts explicit; provider prompt caching not our source cache',
        'trials': trials, 'arms': {phase: aggregate([v for v in trials if v['phase'] == phase]) for phase in phases},
        'complete_primary_paired_ids':paired_ids,
        'complete_primary_pairs':{phase:aggregate([v for v in trials if v['phase']==phase and v['id'] in paired_ids]) for phase in phases[:3]}}
    labels=load(args.quality_file)
    out['quality']=quality(trials,labels,paired_ids)
    out['source_equivalence']=source_equivalence(trials)
    args.output.write_text(json.dumps(out, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(out['arms'], indent=2))


assert canonical('https://EXAMPLE.org/a/#x') == canonical('https://example.org/a')
assert canonical('https://example.org/a?q=1') != canonical('https://example.org/a?q=2')
assert usage([{'cost': .2}, {}])['unknown_cost_calls'] == 1
assert percentile([1, 2, 9], .95) == 9

if __name__ == '__main__': main()
