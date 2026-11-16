"""Paired full-pipeline analysis, with frozen gold and exact proof/source validation."""
import collections,hashlib,json,random,statistics,sys
from html.parser import HTMLParser
from pathlib import Path
ROOT=Path('runs/classifier-relation-2026-10-30')
HOLDOUT = '--holdout' in sys.argv[1:]
PREFIX = 'reliability-holdout' if HOLDOUT else 'classifier-relation'
if HOLDOUT: ROOT=Path('runs/reliability-holdout-2026-10-30')
def load(p):return json.loads(Path(p).read_text())
def sha(b):return hashlib.sha256(b).hexdigest()
def dump(p,v):Path(p).write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def compact(s):return ' '.join(s.split())
cp=Path(f'feasibility/{PREFIX}-corpus-2026-10-30.json')
if cp.exists():
 data=load(cp)
else:
 for p,h in load(ROOT/'gold-freeze.json').items():assert sha(Path(p).read_bytes())==h,('gold/inputs changed',p)
 freeze=ROOT/('code-freeze.json' if HOLDOUT else 'fixed-code-freeze.json')
 for p,h in load(freeze).items():assert sha(Path(p).read_bytes())==h,('code changed since freeze',p)
 data={'pool':load(ROOT/'retrieval/corpus.json'),'gold':load(f'feasibility/{PREFIX}-gold-v1.json'),'branches':{b:load(ROOT/b/'results.json') for b in ['baseline','fixed']},'manifests':{b:load(ROOT/b/'manifest.json') for b in ['baseline','fixed']},'snapshots':{p.name:load(p) for p in (ROOT/'page-cache').glob('*.json') if p.name!='blocks.json'},'audit_documents':{str(p):load(p) for f in ['gold-audit','gold-audit-extra'] for p in (ROOT/f).glob('*-document.json')},'gold_freeze':load(ROOT/'gold-freeze.json'),'code_freeze':load(freeze)}
 dump(cp,data)
if HOLDOUT:
 class GoldText(HTMLParser):
  def __init__(self):super().__init__();self.parts=[]
  def handle_data(self,t):self.parts.append(t)
 key=str(ROOT/'fixed/h07-C011.html')
 if 'gold_html' not in data:
  data['gold_html']={key:Path(key).read_text()};dump(cp,data)
 for row in data['gold']['cases']:
  for g in row['gold']:
   if not g['support']:continue
   if g['snapshot'].endswith('.html'):
    raw=data['gold_html'][g['snapshot']];assert sha(raw.encode())==g['snapshot_sha256']
    parser=GoldText();parser.feed(raw);text=compact(' '.join(parser.parts))
   elif g['snapshot'] in data['audit_documents']:text=compact(data['audit_documents'][g['snapshot']]['text'])
   else:text=compact(data['snapshots'][Path(g['snapshot']).name]['document']['text'])
   for q in g['support']:assert text.encode()[q['start']:q['end']].decode()==q['quote'],'gold source range'
assert data['manifests']['baseline']['classifier']=='conservative-evidence-v3-body' and data['manifests']['baseline']['evidence_version']=='request-evidence-v2'
assert data['manifests']['fixed']['classifier']=='conservative-evidence-v4-bounded-output' and data['manifests']['fixed']['evidence_version']==('request-evidence-v4-audited' if HOLDOUT else 'request-evidence-v3-witness')
assert data['manifests']['fixed']['binary_sha256']!=data['manifests']['baseline']['binary_sha256']
gold={r['id']:{g['id']:g for g in r['gold']} for r in data['gold']['cases']}
pool={r['case']['id']:r for r in data['pool']['cases']}

def metrics(rows):
 tp=fp=fn=tn=unknown=0
 for gs,pred in rows:
  pos={g['id'] for g in gs.values() if g['grade']==2};known={g['id'] for g in gs.values() if g['grade'] is not None}
  tp+=len(pred&pos);fp+=len((pred&known)-pos);fn+=len(pos-pred);tn+=len(known-pos-pred);unknown+=len(pred-known)
 return dict(tp=tp,fp=fp,fn=fn,tn=tn,unjudged_direct=unknown,recall=tp/(tp+fn) if tp+fn else None,precision_judged=tp/(tp+fp) if tp+fp else None)
result={'protocol':{'new_cases':len(pool),'classifier_bypass':False,'same_reranker':'Qwen3-Reranker-0.6B-4bit, oMLX0.7.0','comparison':'full classifier+coverage+Qwen, frozen live SERP; shared URL snapshots/failures; different normal document selection is allowed','gold':data['gold']['protocol'],'timing':'sequential branch replay; raw search not repeated; document snapshot cache and provider cache may differ; not isolated real-time SLA','excluded_trial':'wrong binary identity development run saved separately, NOT baseline; cost/network disclosed separately'},'branches':{},'cases':[]}
samples={};rowsby={b:{r['id']:r for r in data['branches'][b]['cases']} for b in data['branches']}
for mode,rows in rowsby.items():
 samples[mode]=[];returned_samples=[];initial=[];reviews=[];rerank=[];proof_count=0;errors=collections.Counter();initial_valid=0;cachehits=0;reads=0
 for cid,row in rows.items():
  assert row['candidates']==pool[cid]['candidates'] and row['sources']==pool[cid]['sources'],'identical search inputs'
  calls=row['model_calls'];assert len(calls)<=3 and row['budget']['document_reads']<=4 and row['budget']['reranker_calls']<=4
  ic=[c for c in calls if c.get('input',{}).get('messages',[{}])[0].get('content') and 'conservative retrieval reviewer' in c['input']['messages'][0]['content']]
  assert len(ic)==1,'no initial classifier bypass or split in these cases'
  initial+=ic;reviews += [c for c in calls if c not in ic]
  initial_valid+=row['classifier_valid']
  pred={r['id'] for r in row['results'] if r['classification']=='direct'}
  assert len(pred)==row['direct_candidates_predicted'],'no hidden direct outside returned pool'
  samples[mode].append((gold[cid],pred))
  returned={r['id'] for r in row['results']}
  returned_samples.append((gold[cid],returned))
  cs={c['id']:c for c in row['candidates']}
  for r in row['results']:
   if r['id'] not in pred:continue
   if HOLDOUT and mode=='fixed':
    memory=row['page_verification']['evidence_memory'];entry=next(e for e in memory['entries'] if e['id']==r['id'])
    assert entry['audit_supported'] is True and all(c['status']=='supported' for c in entry['checks'][:len(memory['requirements'])]),'direct must pass coverage AND audit'
   for p in r['requirement_evidence']:
    c=cs[r['id']];field=p['field'];raw_hash=None
    if field in ['snippet','title']:text=compact(c[field]);raw_hash=sha(text.encode())
    else:
     snapshot=data['snapshots'][sha(c['url'].encode())+'.json'];doc=snapshot['document'];assert doc and snapshot['record']['final_url']==p['source_url']
     if field=='body':text=compact(doc['text']);raw_hash=doc['text_hash'];assert sha(doc['text'].encode())==raw_hash
     else:text=compact(doc.get('page_title','')[:500]);raw_hash=sha(text.encode())
    assert text.encode()[p['start']:p['end']].decode()==p['quote'],('proof source range',mode,cid,r['id'])
    assert p['text_hash']==raw_hash and p['normalized_text_hash']==sha(text.encode())
    if mode=='fixed' and not p.get('context_only'):assert 16<=len(p['quote'])<=240
    proof_count+=1
  for e in row['errors']:errors[e.split(':')[0]]+=1
  rc=row['page_verification']['passage_reranker']['calls'];rerank+=rc
  reads+=row['budget']['document_reads'];cachehits+=sum(d.get('snapshot_cache_hit',False) for d in row['page_verification']['documents'])
  state=metrics([(gold[cid],pred)]);state.update(direct=sorted(pred),initial_valid=row['classifier_valid'],model_calls=len(calls),reads=row['budget']['document_reads'],status=row['status'],errors=row['errors'])
  state['retrieved_relevant_returned']=metrics([(gold[cid],returned)])['tp']
  target=next((r for r in result['cases'] if r['id']==cid),None)
  if target is None:target={'id':cid,'query':row['query'],'branches':{}};result['cases'].append(target)
  target['branches'][mode]=state
 allcalls=initial+reviews
 stats=metrics(samples[mode]);stats.update(initial_valid=initial_valid,initial_attempts=len(initial),initial_length=sum(c['response']['choices'][0]['finish_reason']=='length' for c in initial),initial_stop_empty=sum(c['response']['choices'][0]['finish_reason']=='stop' and not c['response']['choices'][0]['message'].get('content') for c in initial),initial_reasoning_tokens=sum(c['response']['usage'].get('completion_tokens_details',{}).get('reasoning_tokens',0) or 0 for c in initial),initial_elapsed_ms_median=statistics.median(c['elapsed_ms'] for c in initial),generative_calls=len(allcalls),known_cost_usd=sum(c.get('response',{}).get('usage',{}).get('cost',0) or 0 for c in allcalls),unpriced_generations=sum(not isinstance(c.get('response',{}).get('usage',{}).get('cost'),(int,float)) for c in allcalls),reads=reads,snapshot_cache_hits=cachehits,reranker_calls=len(rerank),reranker_fallbacks=sum(c['status']!='ok' for c in rerank),reranker_pairs=sum(len(c.get('trace',{}).get('pool_ranges',[])) for c in rerank),proof_ranges_validated=proof_count,errors=dict(errors),review_finishes=dict(collections.Counter(c.get('response',{}).get('choices',[{}])[0].get('finish_reason','transport_error') for c in reviews)),providers=dict(collections.Counter(c.get('response',{}).get('provider','unknown') for c in allcalls)))
 result['branches'][mode]=stats
 result['branches'][mode]['all_classes_recall_at10']=metrics(returned_samples)['recall']
 result['branches'][mode]['relevant_returned_at10']=metrics(returned_samples)['tp']
for cid in pool:
 a=next(c for c in result['cases'] if c['id']==cid)['branches'];assert a.keys()=={'baseline','fixed'}
 for mode in a:
  row=rowsby[mode][cid]
  for call in row['model_calls']:
   if 'input' in call:
    text=call['input']['messages'][1]['content'];assert 'gold' not in json.loads(text),'gold leak'
 # The same initial messages/IDs, only budget/reasoning changes.
 a=rowsby['baseline'][cid]['model_calls'][0]['input'];b=rowsby['fixed'][cid]['model_calls'][0]['input'];assert a['messages']==b['messages'] and a['response_format']==b['response_format']
rng=random.Random(20261030);diff=[]
for _ in range(4000):
 chosen=[rng.randrange(len(pool)) for _ in pool];ra=[metrics([samples[m][i] for i in chosen])['recall'] for m in ['baseline','fixed']]
 if None not in ra:diff.append(ra[1]-ra[0])
diff.sort();result['recall_delta_query_bootstrap95']=[diff[int(.025*len(diff))],diff[min(len(diff)-1,int(.975*len(diff)))]]
result['corpus_sha256']=sha(cp.read_bytes());result['original20_sha256']=sha(Path('feasibility/search-cases.json').read_bytes());assert result['original20_sha256']=='b67e3269830ae0e976aa9b4c5dc2f6b9c3e922ad1adda35738289d6b1b8325d6'
dump(f'feasibility/{PREFIX}-results-2026-10-30.json',result)
print(json.dumps(result['branches'],ensure_ascii=False,indent=2));print('recall_delta_ci',result['recall_delta_query_bootstrap95'])
for r in result['cases']:print(r['id'],[(m,b['direct'],b['fp'],b['unjudged_direct']) for m,b in r['branches'].items()])
