"""Recompute the dated frozen benchmark using stdlib only; never calls a model or reads a key."""
import json, hashlib, math, random, statistics, collections, os
from pathlib import Path
ROOT=Path(os.environ.get('WEB_SEARCH_BENCH_CAPTURE_ROOT','runs/model-benchmark-2026-10-30'))
CORPUS=Path('feasibility/model-benchmark-corpus-2026-10-30.json')
data=json.loads(CORPUS.read_text()); cases={r['id']:r for r in data['cases']}
providers={k:json.loads((ROOT/k/'results.json').read_text()) for k in ['deepseek','local']}
index={p:{r['id']:r for r in d['cases']} for p,d in providers.items()}
for provider in providers:
    manifest=json.loads((ROOT/provider/'manifest.json').read_text())
    assert manifest['corpus_sha256']==hashlib.sha256(CORPUS.read_bytes()).hexdigest(), 'corpus/manifest mismatch'

def metrics(rows):
    tp=fp=fn=tn=unjudged=rtp=rfp=runjudged=0; qs=[];recalls=[];p3=p3slots=top10=success=0
    for gold,pred,rank in rows:
        positive={g['id'] for g in gold if g['grade']==2}; judged={g['id'] for g in gold}
        q={'tp':len(pred&positive),'fp':len((pred&judged)-positive),'fn':len(positive-pred),'tn':len(judged-positive-pred)}
        tp+=q['tp'];fp+=q['fp'];fn+=q['fn'];tn+=q['tn'];unjudged+=len(pred-judged)
        if positive:recalls.append(q['tp']/len(positive))
        if rank is not None:
            returned=pred & set(rank[:10]);rtp+=len(returned&positive);rfp+=len((returned&judged)-positive);runjudged+=len(returned-judged)
            known=len(set(rank[:10])&positive);top10+=known;success+=known>=3
            p3+=len(set(rank[:3])&positive);p3slots+=len(rank[:3])
        qs.append(q)
    return {'returned_direct_tp':rtp if p3slots else None,'returned_direct_fp':rfp if p3slots else None,'returned_unjudged_direct':runjudged if p3slots else None,'returned_direct_precision_judged':rtp/(rtp+rfp) if rtp+rfp else None,'queries':len(rows),'tp':tp,'fp':fp,'fn':fn,'tn':tn,'precision':tp/(tp+fp) if tp+fp else None,'recall':tp/(tp+fn) if tp+fn else None,'false_direct_share':fp/(tp+fp) if tp+fp else None,'fpr':fp/(fp+tn) if fp+tn else None,'unjudged_direct':unjudged,'macro_recall_positive_queries':statistics.mean(recalls) if recalls else None,'positive_queries':len(recalls),'known_relevant_top10':top10 if p3slots else None,'top10_success_ge3_lower_bound':success if p3slots else None,'known_relevant_top3_slots':p3 if p3slots else None,'top3_slots':p3slots if p3slots else None}

def predicted(r,stage):
    if not r['valid']:return set()
    return {k for k,v in r['raw_labels'].items() if v=='direct'} if stage=='raw' else {k for k,v in r['initial_labels' if stage=='guarded' else 'final_labels'].items() if v==0}
def rank(r,stage):return r['final_top10' if stage=='body' else 'serp_top10']
def group_result(group,stage,strict=False):
    ids=[r['id'] for r in data['cases'] if r['group'] in group]
    common=[i for i in ids if all(i in index[p] and index[p][i]['valid'] for p in index)]
    def make(p,which):
        rows=[]
        for i in which:
            g=[dict(x) for x in cases[i]['gold']]
            if strict:
                for x in g:
                    if x.get('borderline'):x['grade']=1
            r=index[p].get(i,{'valid':False})
            rows.append((g,predicted(r,stage),(None if stage=='raw' else rank(r,stage)) if r['valid'] else []))
        return rows
    paired={p:metrics(make(p,common)) for p in providers}
    adjusted={p:metrics(make(p,ids)) for p in providers}
    rng=random.Random(20261030); deltas=[];precision_deltas=[];fp_deltas=[]
    # The original RU/EN translations are resampled as paired topic clusters.
    clusters=collections.defaultdict(list)
    for i in common:
        c=cases[i];oid=c['original_id'];key=(c['group'],oid[2:]) if c['group']=='original20_judged_top10' else (c['group'],oid)
        clusters[key].append(i)
    keys=list(clusters)
    for _ in range(2000):
        sample=[i for _ in keys for i in clusters[rng.choice(keys)]] if keys else []
        if not sample:continue
        a=metrics(make('deepseek',sample));b=metrics(make('local',sample))
        if a['recall'] is not None and b['recall'] is not None:deltas.append(a['recall']-b['recall'])
        if a['precision'] is not None and b['precision'] is not None:precision_deltas.append(a['precision']-b['precision'])
        fp_deltas.append((a['fp']-b['fp'])/len(sample))
    deltas.sort();precision_deltas.sort();fp_deltas.sort()
    interval=lambda xs:[xs[int((len(xs)-1)*q)] for q in [.025,.975]] if xs else None
    ci=[deltas[int((len(deltas)-1)*q)] for q in [.025,.975]] if deltas else None
    return {'requested_queries':len(ids),'paired_valid_queries':len(common),'invalid_or_missing_ids':{p:[i for i in ids if i not in index[p] or not index[p][i]['valid']] for p in providers},'paired_valid':paired,'availability_adjusted':adjusted,'recall_delta_deepseek_minus_local_ci95_query_cluster_bootstrap':ci,'precision_delta_ci95':interval(precision_deltas),'false_direct_delta_per_query_ci95':interval(fp_deltas),'uncertainty_note':'Descriptive conditional corpus bootstrap, not independent/population acceptance; original RU/EN pairs clustered.'}

def percentile(a,q):return sorted(a)[int((len(a)-1)*q)] if a else None
availability={}; per_case=[];examples=[]
for p,d in providers.items():
    rs=d['cases'];calls=[c for r in rs for c in r['calls']];usages=[(c.get('response') or {}).get('usage') or {} for c in calls];lat=[r['serp_elapsed_ms'] for r in rs if r['valid']]
    availability[p]={'completed_queries':len(rs),'valid_queries':sum(r['valid'] for r in rs),'invalid_queries':[{'id':r['id'],'errors':r['errors']} for r in rs if not r['valid']],'body_error_cases':[{'id':r['id'],'errors':r['errors']} for r in rs if any(e.startswith('body:') for e in r['errors'])],'total_model_attempts':len(calls),'call_errors':sum('error' in c for c in calls),'serp_latency_ms_valid':{'median':statistics.median(lat) if lat else None,'p95':percentile(lat,.95),'sum':sum(lat)},'initial_call_latency_ms':{'median':statistics.median([c['elapsed_ms'] for r in rs for c in r['calls'][:r['initial_calls']] if 'elapsed_ms' in c]) if any('elapsed_ms' in c for r in rs for c in r['calls'][:r['initial_calls']]) else None,'p95':percentile([c['elapsed_ms'] for r in rs for c in r['calls'][:r['initial_calls']] if 'elapsed_ms' in c],.95)},'initial_model_attempts':sum(r['initial_calls'] for r in rs),'body_model_attempts':sum(len(r['calls'])-r['initial_calls'] for r in rs),'local_reload_ms_total':sum((r.get('lifecycle') or {}).get('elapsed_ms',0) for r in rs),'known_reported_cost_usd':d.get('known_cost_usd'),'unknown_cost_calls':d.get('unknown_cost_calls'),'input_tokens':sum((u.get('prompt_tokens') or 0) for u in usages),'output_tokens':sum((u.get('completion_tokens') or 0) for u in usages),'reasoning_tokens':sum(((u.get('completion_tokens_details') or {}).get('reasoning_tokens') or 0) for u in usages),'provider_histogram':dict(collections.Counter(c.get('response',{}).get('provider','unknown') for c in calls)),'actual_models':dict(collections.Counter(c.get('response',{}).get('model','unknown') for c in calls))}
for i,c in cases.items():
    gold=c['gold'];positive={g['id'] for g in gold if g['grade']==2};judged={g['id'] for g in gold}
    row={'id':i,'query':c['query'],'group':c['group'],'candidate_count':len(c['candidates']),'judged':len(gold),'gold2':len(positive)}
    for p in providers:
        if i not in index[p]:row[p]={'missing':True};continue
        r=index[p][i];row[p]={'valid':r['valid'],'errors':r['errors'],'downgraded':r['downgraded'],'serp_elapsed_ms':r['serp_elapsed_ms'],'body_elapsed_ms':r['body_elapsed_ms'],'calls':len(r['calls']),'body_promotions':len(r['promotions'])}
        for stage in ['raw','guarded','body']:
            pred=predicted(r,stage);row[p][stage]={'tp':len(pred&positive),'fp':len((pred&judged)-positive),'fn':len(positive-pred),'unjudged_direct':len(pred-judged)}
        if r['valid']:
            pred=predicted(r,'body');cand={v['id']:v for v in c['candidates']}
            for kind,ids in [('FP',(pred&judged)-positive),('FN',positive-pred)]:
                for cid in sorted(ids):
                    g=next(g for g in gold if g['id']==cid)
                    examples.append({'provider':p,'case':i,'query':c['query'],'kind':kind,'id':cid,'url':cand[cid]['url'],'title':cand[cid]['title'],'snippet':cand[cid]['snippet'],'gold_basis':g.get('basis','SERP'),'borderline':g.get('borderline',False),'body_attempted':r['body_skipped']=='attempted','guarded_label':r['initial_labels'].get(cid),'body_audit_rejection':r.get('rejected',{}).get(cid)})
    per_case.append(row)
summary={'corpus_sha256':hashlib.sha256(CORPUS.read_bytes()).hexdigest(),'queries':len(cases),'candidates':sum(len(c['candidates']) for c in cases.values()),'judged':sum(len(c['gold']) for c in cases.values()),'gold_origin':data['protocol']['gold'],'availability':availability,'groups':{},'per_case':per_case,'examples':examples,'notes':['No model/prompt tuning after frozen corpus.','Raw-stage ranking metrics are not measured; only binary raw labels are compared.','Invalid initial classifier is excluded from paired-valid quality and contributes no direct to availability-adjusted recall; this is an operational convention, not an irrelevant judgment.','Body replay uses the same fixed cached document universe originally selected in local development; no new web loads and no claim of unbiased full runtime retrieval.']}
for name,groups in [('full_SERP18',['development12_full_SERP','followup6_full_SERP']),('development12',['development12_full_SERP']),('followup6',['followup6_full_SERP']),('mixed8',['recall8_full_mixed_SERP_body']),('original20_top10_judged',['original20_judged_top10'])]:
    summary['groups'][name]={s:group_result(groups,s) for s in ['raw','guarded','body']}
summary['groups']['mixed8_strict']={s:group_result(['recall8_full_mixed_SERP_body'],s,True) for s in ['guarded','body']}
summary['complete']=all(len(d['cases'])==46 for d in providers.values())
summary['predictions']={p:[{k:r[k] for k in ['id','valid','raw_labels','initial_labels','final_labels','serp_top10','final_top10','errors','downgraded','rejected']} for r in d['cases']] for p,d in providers.items()}
summary['verification']={'paired_input':json.loads((ROOT/'paired-input-check.json').read_text()),'metrics_arithmetic':json.loads((ROOT/'metrics-selfcheck.json').read_text()),'source_files_unchanged_since_execution':all(hashlib.sha256((Path('src')/f.name).read_bytes()).hexdigest()==hashlib.sha256(f.read_bytes()).hexdigest() for f in (ROOT/'executed-src').iterdir())}
Path('feasibility/model-benchmark-results-2026-10-30.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'availability':availability,'groups':{k:{s:v['paired_valid'] for s,v in vals.items()} for k,vals in summary['groups'].items()}},ensure_ascii=False,indent=2))
