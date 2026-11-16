"""Blinded snapshot gold. Run BEFORE comparison; never reads branch predictions."""
import hashlib,json
from pathlib import Path

# Substantive exact source anchors, selected by reading captured pages (not model rankings).
component={
'h01':{'C009':['Exceptions raised by finalizer callbacks during garbage collection','A finalizer will never invoke its callback during the later part','each remaining live finalizer is called unless its atexit']},
'h02':{'C006':['this function is susceptible to spurious wakeups','Blocks the current thread until the provided condition becomes false'], 'C007':['this function is susceptible to spurious wakeups','Blocks the current thread until the provided condition becomes false'], 'C002':['Note that this function is susceptible to spurious wakeups','while condition(guard.deref_mut())']},
'h03':{'C007':['will fail but leave behind an “invalid” index','CREATE INDEX command can be performed within a transaction block, but CREATE INDEX CONCURRENTLY cannot']},
'h04':{'C012':['The journal_size_limit pragma may be used to limit the size','if the journal or WAL file is larger it is truncated to the limit'], 'C013':['The checkpoint does not normally truncate the WAL file']},
'h05':{'C008':["When the container stops, the tmpfs mount is removed",'the temporary data may be written to a swap file']},
'h06':{'C003':['neither will shrink below their min-content width'], 'C017':['它们绝不会小于 min-content 的大小'], 'C014':['기본적으로 플렉스 아이템은 콘텐츠의 최소 너비 미만으로 줄어들지 않습니다.']},
'h07':{'C006':['By default, the restore sources for working tree and the index are the index and HEAD respectively','restore both the working tree and the index with --staged --worktree'], 'C007':['By default, if --staged is given, the contents are restored from HEAD','restore both the working tree and the index with --staged --worktree'], 'C009':['Par défaut, si --staged est donné, le contenu est restauré depuis HEAD','restaurer à la fois l’arbre de travail et l’index avec --staged --worktree']},
'h08':{'C005':['A weak entity tag (one prefixed by W/) must not be used in this header']},
}
full={
'r01':{'C005':['Symlink loops are treated like other errors','strict']},
'r02':{'C006':['mutex is poisoned','into_inner']},
'r03':{'C013':['requires an ACCESS EXCLUSIVE lock','requires extra disk space']},
'r04':{'C016':['It is not possible to enable or disable foreign key constraints in the middle of a multi-statement transaction']},
'r05':{'C006':['pre-existing files are obscured by the mount']},
'r06':{'C009':['border-box tells the browser to account for any border and padding']},
'r07':{'C006':['Leave your working tree files and the index unchanged','working tree, which are left untouched']},
'r10':{'C003':['TaskGroup provides stronger safety guarantees than gather','gather will not'], 'C012':['TaskGroup is more powerful: it enforces structured concurrency']},
'r12':{'C012':['requires an ACCESS EXCLUSIVE lock'], 'C013':['requires an exclusive lock on each table'], 'C014':['requires an exclusive lock on each table'], 'C016':['requires an ACCESS EXCLUSIVE lock']},
}

def sha(b):return hashlib.sha256(b).hexdigest()
def build(name,specs):
 root=Path('runs')/(name+'-2026-10-30');corpus=json.loads((root/'capture/corpus.json').read_text());cases=[]
 for row in corpus['cases']:
  gold=[]
  for c in row['candidates']:
   cid=c['id'];p=root/'capture'/f"{row['id']}-{cid}-document.json"
   if not p.exists():p=root/'gold-audit'/p.name
   g={'id':cid,'url':c['url'],'grade':None,'basis':'unjudged; no full captured page','support':[]}
   if p.exists():
    doc=json.loads(p.read_text());raw=doc['text'];text=' '.join(raw.split());assert sha(raw.encode())==doc['text_hash']
    g.update(grade=1,basis='blinded extracted snapshot; incomplete for actual mandatory relationship',snapshot=str(p),text_hash=doc['text_hash'],normalized_text_hash=sha(text.encode()))
    for needle in specs.get(row['id'],{}).get(cid,[]):
     pos=text.find(needle)
     assert pos>=0,(row['id'],cid,needle)
     # A short contiguous context around the selected relationship anchor.
     end=min(len(text),pos+len(needle)+75)
     quote=text[pos:end]
     g['support'].append({'quote':quote,'start':len(text[:pos].encode()),'end':len(text[:end].encode())})
    if g['support']:g.update(grade=2,basis='blinded captured source directly helps the requested relationship; no complete answer required')
   # Partner links and explicit unrelated result cards are known negative, not assumed hidden positives.
   elif 'mate.naver.com' in c['url']:g.update(grade=0,basis='unrelated partner navigation')
   gold.append(g)
  cases.append({'id':row['id'],'query':row['query'],'gold':gold})
 data={'protocol':{'labeler':'single author; blinded to branch outputs, not independent acceptance','grade':'2 relevant,1 observed incomplete,0 unrelated,null unjudged','recall':'known relevant captured-source recall; incomplete pool labels are not universal web recall','unknown_direct':'reported separately, never counted as FP or silently correct','passage_metric':'actual successful runtime-read positives only; any/all exact support anchors inside selected top3 body windows','gold_audit':'extra pages for label verification do not enter branch contexts','full12_novelty':'new strings; five topics overlap earlier development suites; no independent holdout claim'},'cases':cases}
 dest=Path('feasibility')/(name+'-gold-v1.json');dest.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
 files=[dest,root/'capture/corpus.json',Path('feasibility')/('reranker-component-cases-v1.json' if name.endswith('component') else 'reranker-cases-v1.json')]
 (root/'gold-freeze.json').write_text(json.dumps({str(p):sha(p.read_bytes()) for p in files},indent=2))
 print(name,'known',sum(g['grade'] is not None for c in cases for g in c['gold']),'positive',sum(g['grade']==2 for c in cases for g in c['gold']))

if __name__=='__main__':
 build('reranker-component',component)
 build('reranker',full)
