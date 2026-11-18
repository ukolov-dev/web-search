#!/usr/bin/env python3
"""Frozen, serialized MCP search/research evaluation. No retries or source-state reset."""
import argparse, hashlib, importlib.util, json, pathlib, subprocess, time
ROOT = pathlib.Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/search-five-agents-2026-11-18'
CASES = ROOT / 'feasibility/search-five-cases-2026-11-18.json'
spec = importlib.util.spec_from_file_location('existing_probe', ROOT / 'scripts/luna-seven-benchmark.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
read = lambda p: json.loads(p.read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--group', choices=['search', 'research'], required=True)
    parser.add_argument('--live', action='store_true')
    args = parser.parse_args()
    assert args.live, 'Explicit live required'
    freeze = read(RUN / 'freeze.json')
    binary = RUN / 'bin/web-search'
    assert sha(binary) == freeze['binary_sha256']
    assert sha(pathlib.Path(__file__)) == freeze['driver_sha256']
    assert sha(CASES) == freeze['cases_sha256']
    assert sha(RUN / 'gold-preflight.json') == freeze['gold_preflight_sha256']
    assert not (RUN / 'provider-refused.json').exists()
    seed, state = read(RUN / 'seed-ledger.json'), read(RUN / 'source-state.json')
    for k in ['blocked', 'document_blocks']:
        assert all(state[k].get(h) == v for h, v in seed[k].items())
    for case in read(CASES)['cases']:
        assert not (RUN / 'provider-refused.json').exists()
        state = read(RUN / 'source-state.json')
        assert 'seznam' not in state['blocked'], 'No source retry after refusal'
        directory = RUN / args.group / case['id']
        directory.parent.mkdir(exist_ok=True)
        directory.mkdir()  # Existing attempts must never be overwritten.
        time.sleep(2.1)
        command = [str(binary), 'mcp', '--source-state', str(RUN / 'source-state.json'),
                   '--cache-db', str(RUN / (args.group + '-cache.sqlite')),
                   '--capture-dir', str(directory / 'capture')]
        if args.group == 'research':
            command += ['--openrouter-key-file', str(pathlib.Path.home() / '.config/web-search/openrouter.key'),
                        '--openrouter-proxy', 'http://127.0.0.1:10808', '--model', freeze['model'],
                        '--evidence-memory', '--reranker-settings', str(pathlib.Path.home() / '.omlx/settings.json')]
        params = ({'query': case['query'], 'engine': 'seznam', 'max_results': 10}
                  if args.group == 'search' else {'question': case['question'], 'language': case['language']})
        start = time.monotonic()
        with (directory / 'stderr.log').open('w') as err, (directory / 'wire.jsonl').open('w') as wire:
            child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, cwd=ROOT)
            try:
                rpc = probe.shared.Rpc(child, wire)
                rpc.call('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name':'five-agent-eval','version':'1'}}, time.monotonic()+15)
                child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n'); child.stdin.flush()
                tool, _ = rpc.call('tools/call', {'name':args.group, 'arguments':params}, time.monotonic()+210)
                probe.shared.save(directory / 'tool-result.json', tool)
                value = tool.get('structuredContent', {})
                if args.group == 'research':
                    probe.corpus_check(value, directory)
                result = {'status':'completed','tool_status':value.get('status'), 'elapsed_ms':round((time.monotonic()-start)*1000), 'is_error':tool.get('isError'), 'model_refusal':probe.refused(value)}
            except Exception as error:
                result = {'status':'runner_error','error':str(error)[:300]}
            finally:
                child.terminate()
                try: child.wait(timeout=5)
                except subprocess.TimeoutExpired: child.kill();child.wait()
        probe.shared.save(directory / 'summary.json', result)
        probe.shared.save(directory / 'ledger-before.json', state)
        probe.shared.save(directory / 'ledger-after.json', read(RUN / 'source-state.json'))
        print(json.dumps({'id':case['id'],'group':args.group,**result}), flush=True)
        if result.get('model_refusal') or result['status']=='runner_error':
            probe.shared.save(RUN / 'provider-refused.json', result)
            raise SystemExit('Stop after refused/ambiguous failed attempt; no retries')


if __name__ == '__main__':
    main()
