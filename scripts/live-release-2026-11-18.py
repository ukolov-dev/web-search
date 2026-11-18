#!/usr/bin/env python3
"""New immutable live series using the corrected controller, never an old-series resume."""
import argparse
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('source_protocol', ROOT / 'scripts/source-aware-search-2026-11-18.py')
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)
source.shared.RUN = ROOT / 'runs/live-release-2026-11-18'
original_prepare = source.shared.prepare


def prepare(binary):
    def include_wrapper(binary, seed=None, extra_files=()):
        return original_prepare(binary, seed=seed, extra_files=[*extra_files, Path(__file__).resolve()])
    source.shared.prepare = include_wrapper
    source.prepare(binary)


def self_check():
    source.self_check()
    before = {'blocked': {'naver': 'old'}, 'document_url_blocks': {'https://docs.example/bad?q=1': 'timeout'},
              'legacy_timeout_host_exclusions': {'docs.example': {'reason': 'old timeout'}}}
    source.shared.assert_ledger_extends(before, before)
    try:
        source.shared.assert_ledger_extends(before, {'blocked': before['blocked']})
    except AssertionError:
        pass
    else:
        raise AssertionError('URL exclusions or history were silently reset')
    print('PASS: monotonic hard/URL/history ledger; new campaign wrapper included in freeze')


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
        source.shared.run()
    else:
        self_check()
