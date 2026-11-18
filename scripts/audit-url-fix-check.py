#!/usr/bin/env python3
"""Offline regression at the real seed-preparation boundary; no binary/model calls."""
import copy
import hashlib
import importlib.util
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('source_series', ROOT / 'scripts/source-aware-search-2026-11-18.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
paths = [ROOT / 'feasibility/source-aware-search-results-2026-11-18.json',
         ROOT / 'runs/source-aware-search-2026-11-18/source-state.json',
         ROOT / 'runs/primary-search-2026-11-18/source-state.json']
before = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
latest = module.shared.read(paths[0])
with tempfile.TemporaryDirectory() as folder:
    module.shared.RUN = Path(folder)
    captured = {}
    module.shared.prepare = lambda binary, seed=None, extra_files=(): captured.update(seed=copy.deepcopy(seed))
    module.prepare(Path('/unused-binary'))
    seed = captured['seed']
    for host in ['docs.python.org', 'www.pythonpool.com']:
        assert host not in seed['document_blocks'], 'timeout wrongly disables entire host: ' + host
    expected = ['https://docs.python.org/el/3/glossary.html',
                'https://www.pythonpool.com/pythons-itertools-groupby/']
    assert all(url in seed['document_url_blocks'] for url in expected)
    for key in ['blocked', 'document_blocks']:
        for name, reason in latest['seed_ledger'][key].items():
            if name != 'www.pythonpool.com':
                assert seed[key].get(name) == reason, 'hard refusal lost: ' + name
    assert all(host in seed['legacy_timeout_host_exclusions'] for host in ['docs.python.org','www.pythonpool.com'])
# Unproven or changed host refusals must never be downgraded based on old timeout evidence.
prior = module.shared.read(ROOT / 'feasibility/primary-search-results-2026-11-18.json')
changed = copy.deepcopy(latest)
changed['source_state_after']['document_blocks']['docs.python.org'] = 'HTTP 403'
changed['source_state_after']['document_blocks']['unproven.example'] = 'fetch_timeout'
kept = module.prepare_seed(changed, prior)
assert kept['document_blocks']['docs.python.org'] == 'HTTP 403'
assert kept['document_blocks']['unproven.example'] == 'fetch_timeout'
missing = copy.deepcopy(latest)
missing['rows'] = []
assert module.prepare_seed(missing, prior)['document_blocks']['docs.python.org'] == 'fetch_timeout'
again = copy.deepcopy(latest)
again['source_state_after'] = copy.deepcopy(seed)
assert module.prepare_seed(again, prior) == seed
assert module.url_key('https://docs.example/path?q=1#x') == 'https://docs.example/path?q=1'
assert module.url_key('https://docs.example/path?q=2') != module.url_key('https://docs.example/path?q=1')
assert before == {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
print('PASS: proven timeout migration uses exact URLs, hard refusals and immutable history retained')
