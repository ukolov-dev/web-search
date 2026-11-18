#!/usr/bin/env python3
"""A separate frozen campaign: recover document failures, never retry failed hosts."""
import argparse
import copy
import importlib.util
from pathlib import Path
import tempfile
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('primary_series', ROOT / 'scripts/primary-search-2026-11-18.py')
shared = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shared)
shared.RUN = ROOT / 'runs/source-aware-search-2026-11-18'
shared.TEST = 'research::tests::primary_search_experiment::full_native_source_aware_search'


def document_error(reason):
    code = reason.split(':', 1)[0]
    if code in ['source_url_blocked', 'host_disabled', 'fetch_timeout', 'captcha', 'source_blocked', 'rate_limited', 'source_challenge', 'source_policy_changed', 'HTTP 202']:
        return True
    return reason.startswith('HTTP ') and reason[5:].isdigit() and 400 <= int(reason[5:]) < 600


def failure(value, experiment, output):
    if experiment.get('halt') or value.get('error'):
        return 'first_error'
    if value.get('stop_reason') in ['model_error', 'deadline', 'sources_unavailable', 'cancelled']:
        return 'failed_stop'
    for action in value.get('actions', []):
        if action.get('action') == 'read' and action.get('error'):
            if document_error(action['error']):
                continue
            return 'source_or_qwen_error'
        if action.get('error') or (action.get('action') == 'qwen' and action.get('status') != 'ok'):
            return 'source_or_qwen_error'
    if any(c.get('status') != 'returned' for c in value.get('model_calls', [])):
        return 'model_call_error'
    if list(output.glob('request-rejected-*.json')):
        return 'model_refused'
    return None


shared.failure = failure


def url_key(raw):
    url = urlsplit(raw)
    assert url.scheme in ['http', 'https'] and url.hostname and not url.username and not url.password
    return urlunsplit((url.scheme.lower(), url.netloc.lower(), url.path or '/', url.query, ''))


def prepare_seed(latest, prior):
    seed = copy.deepcopy(latest['source_state_after'])
    urls = seed.setdefault('document_url_blocks', {})
    history = seed.setdefault('legacy_timeout_host_exclusions', {})

    def migrate(host, reason, failed_urls, evidence):
        # A timeout-looking string alone cannot authorize removing a domain refusal.
        if seed['document_blocks'].get(host) != reason or not failed_urls:
            return
        for url in failed_urls:
            urls.setdefault(url_key(url), reason)
        history.setdefault(host, {'reason': reason, 'urls': list(failed_urls), 'evidence': evidence})
        del seed['document_blocks'][host]

    for host, reason in latest.get('new_document_exclusions', {}).items():
        failed = [a['url'] for row in latest['rows'] for a in row['result']['actions']
                  if a.get('action') == 'read' and a.get('error') == 'fetch_timeout'
                  and urlsplit(a['url']).hostname == host]
        if reason == 'fetch_timeout':
            migrate(host, reason, failed, 'source-aware-search-results-2026-11-18.json: actual read failure')
    failure = prior.get('source_failure', {})
    if failure.get('reason') == 'fetch_timeout':
        host = urlsplit(failure['url']).hostname
        reason = prior.get('campaign_exclusions', {}).get(host)
        if reason and latest['seed_ledger']['document_blocks'].get(host) == reason:
            migrate(host, reason, [failure['url']], 'primary-search-results-2026-11-18.json: source_failure')
    return seed


def prepare(binary):
    assert not (shared.RUN / 'freeze.json').exists(), 'Old campaign must not be resumed'
    latest = shared.read(ROOT / 'feasibility/source-aware-search-results-2026-11-18.json')
    prior = shared.read(ROOT / 'feasibility/primary-search-results-2026-11-18.json')
    seed = prepare_seed(latest, prior)
    protocol = shared.RUN / 'protocol.json'
    shared.save(protocol, {'document_recoverable': ['fetch_timeout', 'source blocks/challenges', 'HTTP4xx5xx'],
                          'exclusion': 'Transient failures exclude exact initial/redirect URLs, fragments removed; preserve paths/query. Actual403/429/challenges remain failed-host refusals. Legacy timeout-host records retained separately.',
                          'fatal': ['SERP failure', 'Qwen failure', 'model failure/refusal/non-stop finish', 'outer controller timeout'],
                          'scope': 'Same frozen reused NEW4/OLD2 questions/gold/priority/model/caps/order; separate campaign, old series never resumed.'})
    shared.prepare(binary, seed=seed, extra_files=[Path(__file__).resolve(), protocol])


def self_check():
    shared.self_check()
    for reason in ['fetch_timeout', 'source_blocked: previous', 'HTTP 403', 'HTTP 503', 'host_disabled: blocked.example']:
        assert document_error(reason)
    for reason in ['OpenRouter HTTP 429 Too Many Requests', 'Qwen HTTP 429', 'deadline: document read', 'model explicit refusal']:
        assert not document_error(reason)
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)
        assert failure({'actions': [{'action': 'read', 'error': 'fetch_timeout'}]}, {}, path) is None
        assert failure({'actions': [{'action': 'read', 'error': 'source_blocked: campaign host exclusion'}]}, {}, path) is None
        assert failure({}, {'halt': 'OpenRouter HTTP 429'}, path) == 'first_error'
        assert failure({'actions': [{'action': 'serp', 'error': 'fetch_timeout'}]}, {}, path) == 'source_or_qwen_error'
    print('PASS: document-phase recovery only, provider/Qwen/SERP/controller failure fatal')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--prepare', type=Path)
    mode.add_argument('--run', action='store_true')
    mode.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    if args.prepare:
        prepare(args.prepare)
    elif args.run:
        shared.run()
    else:
        self_check()
