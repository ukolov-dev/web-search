#!/usr/bin/env python3
"""Explicit live, LLM-free cache proof over fresh real stdio MCP processes."""
import argparse
import hashlib
import json
from pathlib import Path
import select
import subprocess
import time

ROOT=Path(__file__).resolve().parents[1]
RUN=ROOT/'runs/harness-cache-2026-11-05/cache-mcp-smoke'
URL='https://www.rfc-editor.org/rfc/rfc9111.html'


def request(binary, phase, refresh=False):
    args=[str(binary),'mcp','--cache-db',str(RUN/'proof.sqlite'),'--source-state',str(RUN.parent/'source-state.json')]
    if refresh: args+=['--cache-refresh']
    with (RUN/(phase+'-stderr.log')).open('w') as error:
        child=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=error)
        def call(ordinal, method, params):
            child.stdin.write(json.dumps({'jsonrpc':'2.0','id':ordinal,'method':method,'params':params}).encode()+b'\n');child.stdin.flush()
            deadline=time.monotonic()+45
            while time.monotonic()<deadline:
                readable,_,_=select.select([child.stdout],[],[],1)
                if readable:
                    line=child.stdout.readline()
                    if not line:raise RuntimeError('MCP EOF')
                    frame=json.loads(line)
                    if frame.get('id')==ordinal:
                        if frame.get('error'):raise RuntimeError(str(frame['error']))
                        return frame['result']
            raise TimeoutError('MCP smoke')
        try:
            call(1,'initialize',{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'cache-proof','version':'1'}})
            child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n');child.stdin.flush()
            start=time.monotonic();result=call(2,'tools/call',{'name':'extract','arguments':{'urls':[URL]}})
            result['proof_elapsed_ms']=round((time.monotonic()-start)*1000)
            (RUN/(phase+'.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
            assert not result.get('isError'),result
            return result['structuredContent']['documents'][0]
        finally:
            child.stdin.close()
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired: child.kill();child.wait()


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--live',action='store_true');p.add_argument('--binary',type=Path,required=True);p.add_argument('--run-root',type=Path,default=RUN);a=p.parse_args()
    RUN=a.run_root.resolve()
    assert a.live,'explicit public read permission required'
    RUN.mkdir()
    before=request(a.binary.resolve(),'cold')
    after=request(a.binary.resolve(),'warm')
    assert before['text_hash']==after['text_hash']==hashlib.sha256(after['text'].encode()).hexdigest()
    assert after['source_cache']['hit'] is True and after['cache']['http_reads']==0
    assert after['cache']['extractions']==0 and after['cache']['hits']>=2
    time.sleep(2)
    refreshed=request(a.binary.resolve(),'refresh',True)
    assert refreshed['cache']['http_reads']>=1 and refreshed['source_cache']['hit'] is False
    (RUN/'proof.json').write_text(json.dumps({'restart_warm_body_identical':True,'warm_http_reads':after['cache']['http_reads'],'warm_extractions':after['cache']['extractions'],'refresh_http_reads':refreshed['cache']['http_reads'],'refresh_body_unchanged':before['text_hash']==refreshed['text_hash'],'refresh_fetched_at_unchanged':before['source_cache']['fetched_at']==refreshed['source_cache']['fetched_at']},indent=2)+'\n')
