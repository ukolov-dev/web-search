"""Blinded source gold builder. No branch outputs, labels, or model calls are read."""
import hashlib,json,re
from pathlib import Path
ROOT=Path('runs/classifier-relation-2026-10-30')
SPECS={
's01':{'C007':['--exit-code Make the program exit with codes similar to diff(1).','--quiet Disable all output of the program. Implies --exit-code.'], 'C017':['--quiet','--exit-code'], 'C018':['--exit-code Make the program exit with codes similar to diff(1).','--quiet Disable all output of the program. Implies --exit-code.']},
's02':{'C008':["you need to implement the derive() method",'ExceptionGroups are supported as of python 3.12']},
's03':{'C015':['CREATE TABLE y','insert into y values(null); -- failure'],'C017':['By default, null values in a unique column are not considered equal','The NULLS NOT DISTINCT option modifies this and causes the index to treat nulls as equal.']},
's04':{'C013':['The PRAGMA recursive_triggers setting does not affect the operation of foreign key actions.']},
's05':{'C001':['PID 1 (init process) responsibilities like zombie child reaping and signal forwarding','with --init option, your container’s original entrypoint will be replaced by tini'], 'C005':['--init API 1.25+ Run an init inside the container that forwards signals and reaps processes','You can use the --init flag to indicate that an init process should be used as the PID 1']},
's07':{'C014':['An origin server MUST use the strong comparison function when comparing entity-tags for If-Match','if the field-value is "*", the condition is false if the origin server does not have a current representation']},
's08':{'C001':['Promise.all doesn’t cancel the remaining Promise callbacks when it rejects.'], 'C002':['If one of the promises in the array rejects, Promise.all() immediately rejects the returned promise. The other operations continue to run'], 'C003':['Rejecting the returned promise does not cancel the remaining operations']},
's10':{'C013':['O_NOFOLLOW If the trailing component','Symbolic links in earlier components of the pathname will still be followed.']},
's11':{'C006':['Changed in version 3.12: Prior to Python 3.12, @cached_property included an undocumented lock','In Python 3.12+ this locking is removed.']},
's12':{'C019':['proxy_pass 끝에 / 가 붙는지 여부에 따라','정규식 location에서는 proxy_pass에 URI(trailing slash)를 붙일 수 없다.']},
}
NEGATIVE={'s02':['C005','C010','C013','C014'],'s04':['C001','C004','C005'],'s06':['C001','C009'],'s07':[],'s09':['C016'],'s12':['C020']}
def sha(b):return hashlib.sha256(b).hexdigest()
def load(p):return json.loads(p.read_text())
def document(case,c):
 cache=ROOT/'page-cache'/(sha(c['url'].encode())+'.json')
 if cache.exists():
  d=load(cache).get('document')
  if d:return d,str(cache)
 for folder in ['gold-audit','gold-audit-extra']:
  p=ROOT/folder/f"{case}-{c['id']}-document.json"
  if p.exists():return load(p),str(p)
 return None,None

def build():
 pool=load(ROOT/'retrieval/corpus.json');rows=[]
 for row in pool['cases']:
  case=row['case'];gold=[]
  for c in row['candidates']:
   d,path=document(case['id'],c);g={'id':c['id'],'url':c['url'],'grade':None,'basis':'unjudged source; partial card alone does not establish full-page relevance','support':[]}
   if d:
    raw=d['text'];t=' '.join(raw.split());assert sha(raw.encode())==d['text_hash']
    g.update(snapshot=path,text_hash=d['text_hash'],normalized_text_hash=sha(t.encode()),snapshot_chars=len(t))
    if 500<=len(t)<100000:g.update(grade=1,basis='captured full extracted snapshot does not demonstrate all required relations; source may still help a partial topic')
    for needle in SPECS.get(case['id'],{}).get(c['id'],[]):
     match=re.search(re.escape(needle),t,re.I);assert match,(case['id'],c['id'],needle)
     i=match.start()
     end=min(len(t),i+len(needle)+55);q=t[i:end]
     g['support'].append({'quote':q,'start':len(t[:i].encode()),'end':len(t[:end].encode())})
    if g['support']:g.update(grade=2,basis='direct concrete help for requested relationship in blinded captured source; full answer/tutorial not required')
   if 'mate.naver.com' in c['url'] or c['id'] in NEGATIVE.get(case['id'],[]):g.update(grade=0,basis='explicit unrelated technology/product/navigation in captured title/snippet')
   gold.append(g)
  rows.append({'id':case['id'],'query':case['query'],'gold':gold})
 data={'protocol':{'author':'single author, blinded to baseline/fixed predicted labels; not independent acceptance','freeze':'queries fixed before implementation, source gold before prediction inspection (not before inference); snapshots from sealed trials+runtime union and12audit pages','scope':'known relevant retrieved sources, NOT universal web recall; null=unjudged; unjudged direct separately reported','small_extraction':'<500chars or max100000 limit without positive witness remains unjudged rather than assumed negative','constraints':'explicit relation helps query including negative answer; shared Git common diff options count as relevant; partial topic, API signatures, neighbor API rules do not','query_controls':'s06 failure-order versus load-order ambiguity; s07 If-Match versus If-None-Match; s09 paint containment versus generic clipping','inputs_isolated':'gold source audits are not passed to branches; URL snapshots reached only through each branch normal budgeted decisions'},'cases':rows}
 dest=Path('feasibility/classifier-relation-gold-v1.json');dest.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
 (ROOT/'gold-freeze.json').write_text(json.dumps({str(dest):sha(dest.read_bytes()),'feasibility/classifier-relation-cases-v1.json':sha(Path('feasibility/classifier-relation-cases-v1.json').read_bytes()),str(ROOT/'retrieval/corpus.json'):sha((ROOT/'retrieval/corpus.json').read_bytes())},indent=2))
 print('known',sum(g['grade'] is not None for r in rows for g in r['gold']),'positive',sum(g['grade']==2 for r in rows for g in r['gold']))
if __name__=='__main__':build()
