#!/usr/bin/env python3
"""Package only explicit public benchmark evidence; never bundle runtime homes or dependencies."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import tarfile

ROOT=Path(__file__).resolve().parents[1]
RUN=ROOT/'runs/harness-cache-2026-11-05'
TARGET=ROOT/'feasibility/harness-cache-corpus-2026-11-05.tar.gz'


def selected():
    for phase in ['baseline','cold','warm','recovery-cold','recovery-warm']:
        for trial in sorted((RUN/phase).glob('H*')):
            for name in ['result.json','task.json','events.jsonl','stderr.log']:
                path=trial/name
                if path.exists():yield path
            yield from sorted(trial.glob('agent-llm-*.json'))
            yield from sorted(p for p in (trial/'mcp-capture').rglob('*') if p.is_file())
    for name in ['initial-freeze.json','instrumented-baseline-freeze-v2.json','cohort-freeze.json','metric-boundaries.json','cached-freeze.json','source-state-baseline-end.json','source-state.json','accounting.json']:
        yield RUN/name
    yield RUN/'node-runtime/provenance.json'
    for name in ['post-audit-change.json','final-build.json','auxiliary-costs.json','transport-health.json','implementation.diff']:
        yield RUN/name
    yield RUN/'harness-runtime/package.json'
    yield RUN/'harness-runtime/package-lock.json'
    for folder in ['instrumented-baseline-src','cached-src','final-src','tooling-versions','cache-mcp-smoke','cache-mcp-smoke-final','evaluation']:
        yield from sorted(p for p in (RUN/folder).glob('*') if p.suffix in ['.rs','.py','.mjs','.json','.log'])


def main():
    evaluation=RUN/'evaluation';evaluation.mkdir(exist_ok=True)
    for source,name in [('harness-cache-cases-2026-11-05.json','cases.json'),('harness-cache-quality-2026-11-05.json','quality.json')]:
        (evaluation/name).write_bytes((ROOT/'feasibility'/source).read_bytes())
    files=list(selected())
    assert all(not p.is_symlink() and not {'home','workspace','node_modules'}.intersection(p.relative_to(RUN).parts) for p in files)
    assert len(list((RUN/'baseline').glob('H*/result.json')))==20
    assert len(list((RUN/'cold').glob('H*/result.json')))==20
    assert len(list((RUN/'warm').glob('H*/result.json')))==20
    assert sum(p.stat().st_size for p in files)<256*1024*1024,'evidence package input bound'
    manifest={}
    with TARGET.open('wb') as output,gzip.GzipFile(fileobj=output,mode='wb',mtime=0,filename='') as compressed,tarfile.open(fileobj=compressed,mode='w|') as archive:
        for path in files:
            raw=path.read_bytes();relative=str(path.relative_to(RUN))
            assert b'sk-or-v1-' not in raw,'OpenRouter key-like value in evidence; stop packaging'
            manifest[relative]={'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
            item=tarfile.TarInfo(relative);item.size=len(raw);item.mode=0o600
            archive.addfile(item,io.BytesIO(raw))
        raw=(json.dumps(manifest,indent=2)+'\n').encode();item=tarfile.TarInfo('MANIFEST.json');item.size=len(raw);item.mode=0o600;archive.addfile(item,io.BytesIO(raw))
    assert TARGET.stat().st_size<64*1024*1024,'compressed evidence bound'
    with tarfile.open(TARGET,'r:gz') as archive:
        for member in archive:
            assert member.isfile() and not member.name.startswith('/') and '..' not in Path(member.name).parts
            if member.name=='MANIFEST.json':continue
            assert hashlib.sha256(archive.extractfile(member).read()).hexdigest()==manifest[member.name]['sha256']
    print(json.dumps({'archive':str(TARGET.relative_to(ROOT)),'files':len(files),'bytes':TARGET.stat().st_size,'sha256':hashlib.sha256(TARGET.read_bytes()).hexdigest(),'verified_members':True}))


if __name__=='__main__':main()
