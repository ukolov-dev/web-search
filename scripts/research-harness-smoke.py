#!/usr/bin/env python3
"""One real isolated Harness episode for frozen research development case R06."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import threading
import time
import http.server
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/research-mvp-2026-11-05'
SDK = ROOT / 'runs/harness-cache-2026-11-05'
spec = importlib.util.spec_from_file_location('research_probe', ROOT / 'scripts/research-smoke.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
shared = probe.shared


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--live', action='store_true')
    args = parser.parse_args()
    assert args.live, 'Explicit --live required'
    fixture = ROOT / 'feasibility/research-mvp-confirm-cases-2026-11-05.json'
    assert hashlib.sha256(fixture.read_bytes()).hexdigest() == json.loads((RUN / 'confirm-case-freeze.json').read_text())['sha256']
    case = next(c for c in json.loads(fixture.read_text())['cases'] if c['id'] == 'R06')
    directory = RUN / 'harness' / 'R06'
    directory.mkdir(parents=True, exist_ok=False)
    home, work = directory / 'home', directory / 'workspace'
    home.mkdir(); work.mkdir()
    binary = ROOT / 'target/release/web-search'
    shared.save(directory / 'binary.json', {'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()})
    shutil.copyfile(RUN / 'source-state.json', directory / 'ledger-before.json')
    key = (Path.home() / '.config/web-search/openrouter.key').read_text().strip()
    relay = http.server.ThreadingHTTPServer(('127.0.0.1', 0), shared.Relay)
    relay.token, relay.key, relay.directory = secrets.token_hex(24), key, directory
    relay.calls, relay.refused = 0, False
    relay.opener = urllib.request.build_opener(urllib.request.ProxyHandler({'https': 'http://127.0.0.1:10808'}))
    threading.Thread(target=relay.serve_forever, daemon=True).start()
    ready = directory / 'tools-ready.json'
    mcp_args = ['mcp', '--openrouter-key-file', str(Path.home() / '.config/web-search/openrouter.key'), '--openrouter-proxy', 'http://127.0.0.1:10808',
                '--model', probe.MODEL, '--evidence-memory', '--reranker-settings', str(Path.home() / '.omlx/settings.json'),
                '--source-state', str(RUN / 'source-state.json'), '--capture-dir', str(directory / 'capture'), '--cache-db', str(RUN / 'cache.sqlite')]
    patch = [
        {'id': 'agent-default-model', 'config': {'provider': 'research-openrouter', 'model': probe.MODEL}},
        {'id': 'llm-pi-ai', 'config': {'providers': {'research-openrouter': {'apiKeyEnv': 'HARNESS_RESEARCH_TOKEN', 'api': 'openai-completions',
            'baseURL': f'http://127.0.0.1:{relay.server_port}/v1', 'compat': {'supportsDeveloperRole': False, 'supportsStrictMode': False, 'supportsUsageInStreaming': True, 'maxTokensField': 'max_tokens'},
            'retryPolicy': {'mode': 'normal', 'maxRetries': 0}, 'models': [{'id': probe.MODEL, 'contextWindow': 65536, 'maxTokens': 4096, 'reasoningEfforts': False}]}}}},
        {'id': 'session-title-llm', 'disabled': True},
        {'id': 'system-prompt', 'config': {'includeHarnessIdentity': False, 'includeRuntimeContext': False}},
        {'id': 'sdk-jsonrpc-server', 'config': {'maxTokensAsSuccess': False}},
        {'insert': [{'id': 'research-mcp', 'name': '@deepseek-ai/dsh-mcp-client', 'config': {'serverName': 'web_search', 'transport': 'stdio', 'command': str(binary), 'args': mcp_args,
             'cwd': str(ROOT), 'toolCallTimeoutMs': 210000, 'failOnStartupError': True, 'reconnect': {'enabled': False}}},
            {'id': 'research-only', 'name': str(ROOT / 'scripts/research-harness-guard.mjs'), 'config': {'ready': str(ready)}}]},
    ]
    shared.save(home / 'cordis.patch.yml', patch)
    env = {k: v for k, v in os.environ.items() if not any(s in k.upper() for s in ['API_KEY', 'ACCESS_TOKEN', 'DSH_'])}
    env.update(DSH_HOME=str(home), DSH_TELEMETRY_DISABLED='1', HARNESS_RESEARCH_TOKEN=relay.token, NODE_OPTIONS='--max-old-space-size=768')
    child = None; started = time.monotonic()
    result = {'id': 'R06', 'status': 'error'}
    try:
        with (directory / 'stderr.log').open('w') as stderr, (directory / 'events.jsonl').open('w') as events:
            child = subprocess.Popen([str(SDK / 'node-runtime/node-v24.22.2-darwin-arm64/bin/node'), str(SDK / 'harness-runtime/node_modules/@deepseek-ai/dsh/lib/bin.js'), '--profile', 'sdk'], cwd=work, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
            rpc = shared.Rpc(child, events)
            info, _ = rpc.call('initialize', {'cwd': str(work), 'provider': 'research-openrouter', 'model': probe.MODEL, 'maxTokens': 4096}, time.monotonic()+45)
            result['harness'] = info['serverInfo']
            until = time.monotonic()+30
            while not ready.exists():
                assert child.poll() is None and time.monotonic() < until, 'research MCP discovery failed'
                time.sleep(.1)
            prompt = case['question'] + '\nВызови research ровно один раз с question, в точности равным вопросу выше, language=ru. Сохрани ограничения и сообщи о пропусках.'
            shared.save(directory / 'task.json', {'id': case['id'], 'prompt': prompt})
            _, pending = rpc.call('session/prompt', {'sessionId': 'research-dev-R06', 'contentBlocks': [{'type': 'text', 'text': prompt}]}, time.monotonic()+250)
            deadline = time.monotonic()+250
            calls = []
            while True:
                frame = pending.pop(0) if pending else rpc.read(deadline)
                if frame.get('method') != 'session.event': continue
                event = frame['params']['event']; data = event.get('data', {})
                if event['type'] == 'tool/call': calls.append(data)
                if event['type'] == 'assistant/message': result.setdefault('assistant_messages', []).append(data)
                if event['type'] == 'turn/end':
                    result['reason'] = data.get('reason'); break
            result.update(status='completed' if result.get('reason', {}).get('kind') == 'completed' else 'incomplete', tool_calls=calls,
                          outer_model_calls=relay.calls, elapsed_ms=round((time.monotonic()-started)*1000))
            captures = list((directory / 'capture').glob('*-research'))
            assert len(captures) == 1, 'one completed research tool call required'
            value = json.loads((captures[0] / 'result.json').read_text())
            result['witnesses_checked'] = probe.check(value, captures[0])
            result['research'] = {k: value[k] for k in ['status', 'stop_reason', 'budget', 'answer', 'missing', 'cache']}
    except Exception as error:
        result['error'] = str(error).replace(key, '[redacted]')
    finally:
        if child:
            child.terminate()
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired: child.kill(); child.wait()
        relay.shutdown(); relay.server_close()
        shutil.copyfile(RUN / 'source-state.json', directory / 'ledger-after.json')
        shared.save(directory / 'result.json', result)
        print(json.dumps({k: v for k, v in result.items() if k not in ['assistant_messages', 'research', 'tool_calls']}, ensure_ascii=False))


if __name__ == '__main__':
    main()
