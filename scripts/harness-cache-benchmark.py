#!/usr/bin/env python3
"""Drive the real pinned Harness SDK and an authenticated, fixed OpenRouter transport."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import secrets
import select
import subprocess
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/harness-cache-2026-11-05'
MODEL = 'deepseek/deepseek-v4.1-flash-1017'


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def scrub_event(frame):
    # Thinking is not evidence and is not needed to evaluate the delivered answer.
    if frame.get('method') == 'session.event':
        event = frame.get('params', {}).get('event', {})
        if 'thinking' in event.get('type', ''):
            return {'method': 'redacted-thinking-event'}
    return frame


class Relay(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    def log_message(self, *_):
        pass

    def do_POST(self):
        if self.headers.get('Authorization') != 'Bearer ' + self.server.token or self.path != '/v1/chat/completions':
            self.send_error(403)
            return
        length = int(self.headers.get('Content-Length', '0'))
        if not 0 < length <= 2 * 1024 * 1024:
            self.send_error(413)
            return
        body = json.loads(self.rfile.read(length))
        if body.get('model') != MODEL or self.server.calls >= 6:
            self.send_error(400)
            return
        ordinal = self.server.calls
        self.server.calls += 1
        body['provider'] = {'require_parameters': True, 'allow_fallbacks': False}
        body['reasoning'] = {'effort': 'none'}
        body['temperature'] = 0
        body['max_tokens'] = 4096
        body.pop('thinking', None)
        if body.get('stream'):
            body['stream_options'] = {'include_usage': True}
        started = time.monotonic()
        request = urllib.request.Request('https://openrouter.ai/api/v1/chat/completions',
            data=json.dumps(body).encode(), headers={'Authorization': 'Bearer ' + self.server.key,
            'Content-Type': 'application/json', 'Accept-Encoding': 'identity'})
        record = {'ordinal': ordinal, 'request': body, 'chunks': [], 'usage': None}
        try:
            with self.server.opener.open(request, timeout=90) as response:
                self.send_response(response.status)
                self.send_header('Content-Type', response.headers.get('Content-Type', 'application/json'))
                self.send_header('Connection', 'close')
                self.end_headers()
                record['http_status'] = response.status
                count = 0
                for line in response:
                    count += len(line)
                    if count > 4 * 1024 * 1024:
                        raise RuntimeError('upstream response bound')
                    self.wfile.write(line)
                    self.wfile.flush()
                    if line.startswith(b'data: ') and line.strip() != b'data: [DONE]':
                        chunk = json.loads(line[6:])
                        if chunk.get('usage'):
                            record['usage'] = chunk['usage']
                        record['chunks'].append(chunk)
                self.close_connection = True
        except urllib.error.HTTPError as error:
            raw = error.read(65536).decode(errors='replace').replace(self.server.key, '[redacted]')
            record.update(http_status=error.code, error=raw)
            if error.code in (403, 429):
                self.server.refused = True
            self.send_response(error.code)
            self.send_header('Content-Length', str(len(raw.encode())))
            self.end_headers()
            self.wfile.write(raw.encode())
        except Exception as error:
            record['error'] = type(error).__name__
            record['error_detail'] = str(error).replace(self.server.key, '[redacted]')[:500]
            self.close_connection = True
        finally:
            record['elapsed_ms'] = round((time.monotonic() - started) * 1000)
            save(self.server.directory / f'agent-llm-{ordinal:02}.json', record)


class Rpc:
    def __init__(self, child, events):
        self.child, self.events = child, events
        self.buffer = b''
        self.ordinal = 0

    def read(self, deadline):
        while b'\n' not in self.buffer:
            if time.monotonic() >= deadline:
                raise TimeoutError('Harness turn deadline')
            ready, _, _ = select.select([self.child.stdout], [], [], min(1, deadline - time.monotonic()))
            if not ready:
                continue
            chunk = os.read(self.child.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError('Harness stdout EOF')
            self.buffer += chunk
            if len(self.buffer) > 8 * 1024 * 1024:
                raise RuntimeError('Harness frame limit')
        raw, self.buffer = self.buffer.split(b'\n', 1)
        frame = json.loads(raw)
        self.events.write(json.dumps(scrub_event(frame), ensure_ascii=False) + '\n')
        self.events.flush()
        return frame

    def call(self, method, params, deadline):
        self.ordinal += 1
        self.child.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': self.ordinal, 'method': method, 'params': params}).encode() + b'\n')
        self.child.stdin.flush()
        pending = []
        while True:
            frame = self.read(deadline)
            if frame.get('id') == self.ordinal:
                if frame.get('error'):
                    raise RuntimeError('Harness RPC rejected: ' + json.dumps(frame['error']))
                return frame['result'], pending
            pending.append(frame)


def run_case(case, phase, binary, cache_db=None, *, run_root=None, guard=None):
    run_root = RUN if run_root is None else run_root
    directory = run_root / phase / case['id']
    directory.mkdir(parents=True)
    assert not (directory / 'result.json').exists(), 'Never overwrite a trial'
    home, work = directory / 'home', directory / 'workspace'
    home.mkdir(); work.mkdir()
    key = (Path.home() / '.config/web-search/openrouter.key').read_text().strip()
    token = secrets.token_hex(24)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Relay)
    server.token, server.key, server.directory = token, key, directory
    server.calls, server.refused = 0, False
    server.opener = urllib.request.build_opener(urllib.request.ProxyHandler({'https': 'http://127.0.0.1:10808'}))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    ready = directory / 'tools-ready.json'
    args = ['mcp', '--openrouter-key-file', str(Path.home() / '.config/web-search/openrouter.key'),
        '--openrouter-proxy', 'http://127.0.0.1:10808', '--model', MODEL, '--evidence-memory',
        '--reranker-settings', str(Path.home() / '.omlx/settings.json'), '--capture-dir', str(directory / 'mcp-capture')]
    args += ['--source-state', str(run_root / 'source-state.json')]
    if cache_db:
        args += ['--cache-db', str(cache_db)]
    patch = [
        {'id': 'agent-default-model', 'config': {'provider': 'benchmark-openrouter', 'model': MODEL}},
        {'id': 'llm-pi-ai', 'config': {'providers': {'benchmark-openrouter': {
            'apiKeyEnv': 'HARNESS_BENCH_TOKEN', 'api': 'openai-completions',
            'baseURL': f'http://127.0.0.1:{server.server_port}/v1',
            'compat': {'supportsDeveloperRole': False, 'supportsStrictMode': False, 'supportsUsageInStreaming': True, 'maxTokensField': 'max_tokens'},
            'retryPolicy': {'mode': 'normal', 'maxRetries': 0},
            'models': [{'id': MODEL, 'contextWindow': 65536, 'maxTokens': 4096, 'reasoningEfforts': False}]}}}},
        {'id': 'session-title-llm', 'disabled': True},
        {'id': 'system-prompt', 'config': {'includeHarnessIdentity': False, 'includeRuntimeContext': False}},
        {'id': 'sdk-jsonrpc-server', 'config': {'maxTokensAsSuccess': False}},
        {'insert': [
            {'id': 'benchmark-mcp', 'name': '@deepseek-ai/dsh-mcp-client', 'config': {
                'serverName': 'web_search', 'transport': 'stdio', 'command': str(binary), 'args': args,
                'cwd': str(ROOT), 'toolCallTimeoutMs': 210000, 'failOnStartupError': True,
                'reconnect': {'enabled': False}}},
            {'id': 'benchmark-search-only', 'name': str(guard or ROOT / 'scripts/harness-cache-guard.mjs'), 'config': {'ready': str(ready)}}]},
    ]
    save(home / 'cordis.patch.yml', patch)
    env = {k: v for k, v in os.environ.items() if not any(s in k.upper() for s in ['API_KEY', 'ACCESS_TOKEN', 'DSH_'])}
    env.update(DSH_HOME=str(home), DSH_TELEMETRY_DISABLED='1', HARNESS_BENCH_TOKEN=token, NODE_OPTIONS='--max-old-space-size=768')
    node = RUN / 'node-runtime/node-v24.22.2-darwin-arm64/bin/node'
    cli = RUN / 'harness-runtime/node_modules/@deepseek-ai/dsh/lib/bin.js'
    begin = time.monotonic(); result = {'id': case['id'], 'phase': phase, 'status': 'error'}
    child = None
    try:
        with (directory / 'stderr.log').open('w') as stderr, (directory / 'events.jsonl').open('w') as events:
            child = subprocess.Popen([str(node), str(cli), '--profile', 'sdk'], cwd=work, env=env,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
            rpc = Rpc(child, events)
            initialized, _ = rpc.call('initialize', {'cwd': str(work), 'provider': 'benchmark-openrouter', 'model': MODEL, 'maxTokens': 4096}, time.monotonic() + 45)
            assert initialized['serverInfo']['name'] == 'deepseek-harness-sdk-runtime'
            result['harness'] = initialized['serverInfo']
            deadline = time.monotonic() + 30
            while not ready.exists():
                if child.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError('Harness MCP discovery did not become ready')
                time.sleep(.1)
            result['discovered_tools'] = json.loads(ready.read_text())
            result['startup_ms'] = round((time.monotonic() - begin) * 1000)
            prompt = case['prompt']
            if 'query' in case:
                prompt += '\nНачни с ranked_search с query ровно ' + json.dumps(case['query'], ensure_ascii=False) + ', max_results=5. Прочитай до двух наиболее полезных источников через extract; если источников недостаточно, сообщи об этом. Дополнительный search допускается только с engine seznam или naver. До4последовательных вызовов инструментов. Ответ до180слов, ссылки и короткие source-цитаты.'
            save(directory / 'task.json', {'id': case['id'], 'prompt': prompt})
            started = time.monotonic(); deadline = started + 360
            accepted, pending = rpc.call('session/prompt', {'sessionId': case['id'] + '-' + phase,
                'contentBlocks': [{'type': 'text', 'text': prompt}]}, deadline)
            answer, tool_calls, reason = '', [], None
            while True:
                frame = pending.pop(0) if pending else rpc.read(deadline)
                if frame.get('method') != 'session.event':
                    continue
                event = frame['params']['event'];data = event.get('data', {})
                if event['type'] == 'tool/call':
                    tool_calls.append(data)
                elif event['type'] == 'assistant/message':
                    result.setdefault('assistant_messages', []).append(data)
                elif event['type'] == 'turn/end':
                    reason = data.get('reason');break
            result.update(status='completed' if reason and reason.get('kind') == 'completed' else 'incomplete',
                reason=reason, tool_calls=tool_calls, task_elapsed_ms=round((time.monotonic() - started) * 1000),
                outer_model_calls=server.calls, refusal=server.refused)
    except Exception as error:
        result['error'] = str(error).replace(key, '[redacted]')
    finally:
        if child:
            child.terminate()
            try: child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill();child.wait()
        server.shutdown();server.server_close()
        result['process_elapsed_ms'] = round((time.monotonic() - begin) * 1000)
        save(directory / 'result.json', result)
    return result


assert scrub_event({'method': 'session.event', 'params': {'event': {'type': 'thinking/delta'}}}) == {'method': 'redacted-thinking-event'}

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--phase', required=True)
    parser.add_argument('--case', required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--cache-db', type=Path)
    parser.add_argument('--live', action='store_true')
    args = parser.parse_args()
    assert args.live, 'Explicit --live required; real model/tool calls'
    if args.case == 'smoke':
        case = {'id': 'smoke', 'prompt': 'Проверь MCP: вызови web_search.extract для http://127.0.0.1/ и кратко объясни полученный отказ. Это проверка запрета приватных URL. Другие инструменты не нужны.'}
    else:
        case = next(c for c in json.loads((ROOT / 'feasibility/harness-cache-cases-2026-11-05.json').read_text())['cases'] if c['id'] == args.case)
    result = run_case(case, args.phase, args.binary.resolve(), args.cache_db.resolve() if args.cache_db else None)
    print(json.dumps({k: v for k, v in result.items() if k in ['id', 'phase', 'status', 'error', 'reason', 'outer_model_calls', 'task_elapsed_ms']}), flush=True)
    raise SystemExit(0 if result['status'] == 'completed' else 1)
