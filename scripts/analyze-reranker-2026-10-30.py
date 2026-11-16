"""Freeze-preserving paired analysis. stdlib only; never calls models or reads keys."""
import collections, hashlib, json, random, statistics
from pathlib import Path

def sha(b):return hashlib.sha256(b).hexdigest()
def load(p):return json.loads(Path(p).read_text())
def dump(p,x):Path(p).write_text(json.dumps(x,ensure_ascii=False,indent=2)+'\n')
def compact(t):return ' '.join(t.split())
def direct(branch):return {c for c,k in branch['labels'].items() if k==0}
def metrics(rows):
 tp=fp=fn=tn=unknown=0
 for gs,pred in rows:
  known={g['id'] for g in gs if g['grade'] is not None};pos={g['id'] for g in gs if g['grade']==2}
  tp+=len(pred&pos);fp+=len((pred&known)-pos);fn+=len(pos-pred);tn+=len(known-pos-pred);unknown+=len(pred-known)
 return dict(tp=tp,fp=fp,fn=fn,tn=tn,unjudged_direct=unknown,recall=tp/(tp+fn) if tp+fn else None,precision_judged=tp/(tp+fp) if tp+fp else None)

corpus_path=Path('feasibility/reranker-corpus-2026-10-30.json')
if corpus_path.exists():
 bundle=load(corpus_path)
 previous=Path('feasibility/reranker-results-2026-10-30.json')
 if previous.exists():assert sha(corpus_path.read_bytes())==load(previous)['corpus_sha256'], 'durable corpus changed'
else:
 bundle={'date':'2026-10-30','model':load('runs/reranker-2026-10-30/model-manifest.json'),'sets':{}}
 for label,name in [('component','reranker-component'),('full','reranker')]:
  root=Path('runs')/(name+'-2026-10-30');freeze=load(root/'gold-freeze.json')
  for p,h in freeze.items():assert sha(Path(p).read_bytes())==h,('freeze changed',p)
  docs={str(p):load(p) for folder in ['capture','gold-audit'] for p in (root/folder).glob('*-document.json')}
  bundle['sets'][label]={'capture':load(root/'capture/corpus.json'),'comparison':load(root/'comparison/results.json'),'gold':load('feasibility/'+name+'-gold-v1.json'),'documents':docs,'freeze':freeze}
 dump('feasibility/reranker-corpus-2026-10-30.json',bundle)

result={'protocol':{'comparison':'coverage lexical vs coverage local Qwen, not legacy default','full':'original classifier preserved; 5/12 valid; 5 topics overlap earlier suites','component':'8 fresh distinct-mechanism queries; provisional background upstream, not end-to-end','known_recall':'retrieved known relevant captured sources only; unjudged separately','passage_hit':'successful runtime-read positive pages with at least one complete gold anchor in top3','all_support':'every annotated gold anchor contained in some top3 window','timing':'shared model+document elapsed + replay branch wall, excluding retrieval and cache I/O gap; diagnostic, not isolated end-to-end latency','concurrency':'full and component comparison jobs overlapped; local/server/provider contention possible','gold_author':'single author blinded to branch outputs; neither independent holdout acceptance nor universal corpus labels'},'sets':{}}
for label,data in bundle['sets'].items():
 capture=data['capture'];base={r['id']:r for r in capture['cases']};gold={r['id']:r for r in data['gold']['cases']};comp=data['comparison']['cases']
 assert base.keys()==gold.keys()=={r['id'] for r in comp}
 summary={'queries':len(comp),'candidates':sum(len(r['candidates']) for r in base.values()),'known_judgments':sum(g['grade'] is not None for r in gold.values() for g in r['gold']),'initial_valid':sum(r['initial_valid'] for r in base.values()),'case_results':[]}
 samples={m:[] for m in ['lexical','qwen']};passage={m:[] for m in samples};latency={m:[] for m in samples};rerank=[];calls=[];sources=[]
 for row in comp:
  cid=row['id'];g=gold[cid]['gold'];b=base[cid];s= row['branches'];assert s['lexical']['memory']['requirements']==s['qwen']['memory']['requirements']
  assert s['lexical']['valid']==s['qwen']['valid']
  goldby={x['id']:x for x in g};docs={d['id']:d for d in b['documents'] if 'text_hash' in d};states={}
  for mode,branch in s.items():
   pred=direct(branch);samples[mode].append((g,pred));latency[mode].append(branch['logical_ms']);calls+=branch['calls']
   hits=allhits=eligible=anchors=anchorhits=0
   for entry in branch['memory']['entries']:
    body=[p for p in entry['passages'] if p['field']=='body'];assert len(body)<=3
    # No generated body or location drift can influence reported quality.
    if entry['id'] in docs:
     path=next(p for p in data['documents'] if p.endswith(f'{cid}-{entry["id"]}-document.json') and '/capture/' in p)
     doc=data['documents'][path];text=compact(doc['text']).encode();assert sha(doc['text'].encode())==doc['text_hash']
     for p in body:
      assert text[p['start']:p['end']].decode()==p['text'] and p['text_hash']==doc['text_hash'] and p['normalized_text_hash']==sha(text)
     gg=goldby[entry['id']]
     if gg['grade']==2 and gg['support']:
      found=[any(p['start']<=a['start'] and p['end']>=a['end'] for p in body) for a in gg['support']]
      eligible+=1;hits+=any(found);allhits+=all(found);anchors+=len(found);anchorhits+=sum(found)
   passage[mode].append((hits,allhits,eligible,anchorhits,anchors))
   states[mode]={'direct':sorted(pred),'judged':metrics([(g,pred)]),'passage_hit':hits,'passage_all':allhits,'passage_eligible':eligible,'logical_ms':branch['logical_ms'],'errors':branch['errors']}
   rerank+=branch['reranker_calls']
  first=lambda mode:{e['id']:[p for p in e['passages'] if p['field'] in ['title','snippet']] for e in s[mode]['memory']['entries']}
  assert first('lexical')==first('qwen')
  summary['case_results'].append({'id':cid,'query':b['query'],'branches':states})
 for mode in samples:
  t=metrics(samples[mode]);p=passage[mode];h,a,n,ah,an=[sum(x[i] for x in p) for i in range(5)]
  t.update(passage_hits=h,passage_all=a,passage_eligible=n,passage_hit_at3=h/n if n else None,passage_all_at3=a/n if n else None,anchor_hits=ah,anchor_count=an,anchor_recall_at3=ah/an if an else None,logical_ms_median=statistics.median(latency[mode]),logical_ms_mean=statistics.mean(latency[mode]))
  summary[mode]=t
 # Paired query bootstrap, not source-iid confidence.
 rng=random.Random(20261030);deltas=[]
 for _ in range(4000):
  chosen=[rng.randrange(len(comp)) for _ in comp];rec=[metrics([samples[m][i] for i in chosen])['recall'] for m in ['lexical','qwen']]
  if None not in rec:deltas.append(rec[1]-rec[0])
 deltas.sort();summary['recall_delta_95pct_query_bootstrap']=[deltas[int(.025*len(deltas))],deltas[min(len(deltas)-1,int(.975*len(deltas)))]]
 times=[c['trace']['call']['elapsed_ms'] for c in rerank if c['status']=='ok']
 summary['reranker']={'calls':len(rerank),'fallbacks':sum(c['status']!='ok' for c in rerank),'pairs':sum(len(c.get('trace',{}).get('pool_ranges',[])) for c in rerank),'elapsed_ms_median':statistics.median(times) if times else None,'elapsed_ms_total':sum(times),'tokens':sum(c.get('trace',{}).get('call',{}).get('response',{}).get('usage',{}).get('total_tokens',0) for c in rerank)}
 capture_calls=[c for b in base.values() for k in ['initial_calls','first_calls'] for c in b[k]]
 summary['physical_generations']=len(capture_calls)+len(calls)
 summary['known_cost_usd']=capture['cost_usd']+data['comparison']['review_cost_usd']
 summary['source_attempts']=sum(len(b['sources']) for b in base.values());summary['runtime_read_attempts']=sum(len(b['documents']) for b in base.values());summary['runtime_documents']=sum('text_hash' in d for b in base.values() for d in b['documents'])
 summary['source_errors']=dict(collections.Counter(s['error'] or 'ok' for b in base.values() for s in b['sources']))
 result['sets'][label]=summary
result['original20_sha256']=sha(Path('feasibility/search-cases.json').read_bytes());assert result['original20_sha256']=='b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6'
result['corpus_sha256']=sha(Path('feasibility/reranker-corpus-2026-10-30.json').read_bytes())
dump('feasibility/reranker-results-2026-10-30.json',result)
for name,s in result['sets'].items():print(name,json.dumps({k:v for k,v in s.items() if k!='case_results'},ensure_ascii=False,indent=2))
