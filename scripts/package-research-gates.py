#!/usr/bin/env python3
"""Verify and package explicit research evidence; never include SDK homes, keys or caches."""
import gzip
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import statistics
import tarfile

ROOT=Path(__file__).resolve().parents[1]
RUN=ROOT/'runs/research-gates-2026-11-06'
PREFIX='research-gates'
DATE='2026-11-06'
spec=importlib.util.spec_from_file_location('gate_probe',ROOT/'scripts/research-gates-benchmark.py')
probe=importlib.util.module_from_spec(spec);spec.loader.exec_module(probe)


def read(path):
    return json.loads(path.read_text())


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def main():
    probe.self_check()
    quality=read(ROOT/f'feasibility/{PREFIX}-quality-{DATE}.json')
    phases={'baseline':{'new':12,'confirm':12,'final':12,'harness':2},'candidate':{'new':12},'candidate-v2':{'confirm':12,'old':2,'harness':2},'candidate-v3':{'final':12,'old':2,'harness':2},'candidate-v4':{'final':12,'old':2,'harness':2}}
    cases=[];files=[];witnesses=ranges=versions=0
    for arm,groups in phases.items():
        for group,expected in groups.items():
            trials=sorted((RUN/arm/group).glob('*/summary.json'))
            assert len(trials)==expected,(arm,group,len(trials))
            for trial in trials:
                directory=trial.parent;summary=read(trial)
                assert summary['status']=='completed',(arm,group,directory.name,summary)
                capture=next((directory/('mcp-capture' if group=='harness' else 'capture')).glob('*-research'))
                value=read(capture/'result.json')
                witnesses+=probe.probe.probe.check(value,capture)
                ranges+=len(value['claims'])
                docs={(d['url'],sha(d['text'].encode())):d for d in map(read,capture.glob('document-*.json'))}
                for source in value['sources']:
                    doc=docs[(source['url'],source['text_hash'])]
                    raw=json.dumps(['research-extract-v1',source['url'],source['text_hash'],doc['metadata']],ensure_ascii=False,separators=(',',':'),sort_keys=True).encode()
                    assert sha(raw)==source['document_version']
                    versions+=1
                calls=value['model_calls']
                cost=sum((c.get('usage') or {}).get('cost') or 0 for c in calls)
                unknown=sum(c.get('cost_known') is False or not isinstance((c.get('usage') or {}).get('cost'),(int,float)) for c in calls)
                outer=[read(p) for p in directory.glob('agent-llm-*.json')]
                outer_cost=sum((c.get('usage') or {}).get('cost') or 0 for c in outer)
                case={'arm':arm,'cohort':group,'id':directory.name,'status':value['status'],'stop_reason':value['stop_reason'],'elapsed_ms':summary.get('task_elapsed_ms',summary.get('elapsed_ms')),'research_elapsed_ms':value['elapsed_ms'],'budget':value['budget']['used'],'cache':value['cache'],'cost_usd_known':cost,'unknown_cost_calls':unknown,'model_calls':len(calls),'outer_calls':len(outer),'outer_cost_usd_known':outer_cost,'claims':[{k:c[k] for k in ['id','text','kind','facets']} for c in value['claims']],'sources':[{k:s[k] for k in ['id','url','text_hash','document_version','eligible']} for s in value['sources']],'public_bytes':len(json.dumps(value,ensure_ascii=False,separators=(',',':')).encode()),'raw_prefix':str(directory.relative_to(RUN))}
                cases.append(case)
                for name in ['summary.json','result.json','task.json','events.jsonl','stderr.log','wire.jsonl','tool-result.json','discovery.json','ledger-before.json','ledger-after.json']:
                    if (directory/name).is_file():files.append((directory/name,str((directory/name).relative_to(RUN))))
                files.extend((p,str(p.relative_to(RUN))) for p in sorted(directory.glob('agent-llm-*.json')))
                files.extend((p,str(p.relative_to(RUN))) for p in sorted(capture.rglob('*')) if p.is_file())
    cohort_stats=[]
    for arm,groups in phases.items():
        for group in groups:
            values=[c for c in cases if c['arm']==arm and c['cohort']==group]
            cohort_stats.append({'arm':arm,'cohort':group,'completed':len(values),'claims':sum(len(c['claims']) for c in values),'ok':sum(c['status']=='ok' for c in values),'latency_p50_ms':statistics.median(c['elapsed_ms'] for c in values),'cost_usd_known':sum(c['cost_usd_known'] for c in values),'outer_cost_usd_known':sum(c['outer_cost_usd_known'] for c in values),'unknown_cost_calls':sum(c['unknown_cost_calls'] for c in values),'source_http_reads':sum(c['cache']['http_reads'] for c in values),'qwen_http_calls':sum(c['cache']['qwen_calls'] for c in values),'cache_hits':sum(c['cache']['hits'] for c in values),'model_calls':sum(c['model_calls'] for c in values)})
    seed=read(RUN/'seed-ledger.json');state=read(RUN/'source-state.json')
    assert all(k in state['blocked'] for k in seed['blocked'])
    assert all(k in state['document_blocks'] for k in seed['document_blocks'])
    snapshots={arm:read(RUN/f'{arm}.json') for arm in phases}
    snapshots['baseline']['sources']={str(Path(p).relative_to(ROOT)) if Path(p).is_absolute() else p:h for p,h in snapshots['baseline']['sources'].items()}
    for path,digest in snapshots['candidate-v4']['sources'].items():assert sha((ROOT/path).read_bytes())==digest,path
    for f,k in [('search-cases.json','original20_sha256'),('b17-policy.json','b17_sha256')]:assert sha((ROOT/'feasibility'/f).read_bytes())==snapshots['baseline'][k]
    results={'schema':'research-gates-results-v1','client_date':DATE,'snapshots':snapshots,'cohort_stats':cohort_stats,'episodes':cases,'quality':quality,'provenance':{'captures':len(cases),'answer_ranges':ranges,'exact_witness_slices':witnesses,'source_versions':versions,'all_passed':True},'source_state_after':state,'source_refusals_preserved':True,'limits':{'all_cohorts_separate':True,'cache_and_source_drift_confound_latency':True,'no_general_qualification':True,'no_release_or_deploy':True,'v3_superseded_by_blind_independent_review_before_opening_D_predictions':True},'cost_usd_known_all_inner':sum(c['cost_usd_known'] for c in cases),'cost_usd_known_all_outer':sum(c['outer_cost_usd_known'] for c in cases)}
    output=ROOT/f'feasibility/{PREFIX}-results-{DATE}.json';probe.shared.save(output,results)
    for name in ['baseline.json','candidate.json','candidate-v2.json','candidate-v3.json','candidate-v4.json','seed-ledger.json','source-state.json','confirmation-freeze.json','final-freeze.json']:
        files.append((RUN/name,'snapshots/'+name))
    for p in sorted(RUN.glob('*.log')):files.append((p,'checks/'+p.name))
    for p in sorted((RUN/'old-r03-live-audit').glob('*.json')):files.append((p,'old-component/'+p.name))
    for p in [output,ROOT/f'feasibility/{PREFIX}-quality-{DATE}.json',*[ROOT/f'feasibility/{PREFIX}-{x}cases-{DATE}.json' for x in ['','confirm-','final-']]]:files.append((p,'evaluation/'+p.name))
    for p in [ROOT/'src/research.rs',ROOT/'src/research_tests.rs',ROOT/'tests/fixtures/research/old-luna-gates.json',ROOT/'scripts/research-gates-benchmark.py',Path(__file__)]:files.append((p,'source/'+str(p.relative_to(ROOT))))
    assert len(files)==len({name for _,name in files})
    assert all(not p.is_symlink() and not {'home','workspace','node_modules','bin'}.intersection(p.relative_to(ROOT).parts) for p,_ in files)
    secrets=[(Path.home()/'.config/web-search/openrouter.key').read_bytes().strip()]
    local=read(Path.home()/'.omlx/settings.json').get('auth',{}).get('api_key')
    if local:secrets.append(local.encode())
    target=ROOT/f'feasibility/{PREFIX}-corpus-{DATE}.tar.gz';manifest={}
    with target.open('wb') as stream,gzip.GzipFile(fileobj=stream,mode='wb',mtime=0,filename='') as compressed,tarfile.open(fileobj=compressed,mode='w|') as archive:
        for path,name in files:
            raw=path.read_bytes();assert all(not secret or secret not in raw for secret in secrets),'credential in evidence'
            manifest[name]={'bytes':len(raw),'sha256':sha(raw)}
            member=tarfile.TarInfo(name);member.size=len(raw);member.mode=0o600;archive.addfile(member,io.BytesIO(raw))
        raw=(json.dumps(manifest,indent=2)+'\n').encode();member=tarfile.TarInfo('MANIFEST.json');member.size=len(raw);member.mode=0o600;archive.addfile(member,io.BytesIO(raw))
    with tarfile.open(target,'r:gz') as archive:
        for member in archive:
            assert member.isfile() and not member.name.startswith('/') and '..' not in Path(member.name).parts
            if member.name!='MANIFEST.json':assert sha(archive.extractfile(member).read())==manifest[member.name]['sha256']
    package={'schema':'research-gates-package-v1','archive':str(target.relative_to(ROOT)),'bytes':target.stat().st_size,'sha256':sha(target.read_bytes()),'regular_members_including_manifest':len(files)+1,'all_member_hashes_verified':True,'exact_private_credentials_detected':0,'excluded':['binaries','SDK homes/config/workspace','SQLite/cache/vendor','credentials'],'not_standalone_installer':True}
    probe.shared.save(ROOT/f'feasibility/{PREFIX}-package-{DATE}.json',package)
    print(json.dumps({'cohorts':cohort_stats,'provenance':results['provenance'],'package':package},ensure_ascii=False,indent=2))


if __name__=='__main__':main()
