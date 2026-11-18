#!/usr/bin/env python3
"""Reuse the frozen stdio runner; preserve one refusal ledger across all phases."""
import argparse
import hashlib
import importlib.util
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUN = ROOT / 'runs/search-repair-2026-11-18'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--phase', choices=['baseline', 'candidate', 'regression'], required=True)
    parser.add_argument('--group', choices=['search', 'research'], required=True)
    parser.add_argument('--live', action='store_true', required=True)
    args = parser.parse_args()
    manifest = json.loads((RUN / 'manifest.json').read_text())
    assert hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest() == manifest['wrapper_sha256']
    directory = RUN / args.phase
    assert (directory / 'source-state.json').resolve() == (RUN / 'source-state.json').resolve()
    spec = importlib.util.spec_from_file_location('frozen_runner', ROOT / 'scripts/search-five-agents-2026-11-18.py')
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    driver.RUN = directory
    driver.CASES = ROOT / manifest['cases'][args.phase]
    sys.argv = [str(spec.origin), '--group', args.group, '--live']
    driver.main()


if __name__ == '__main__':
    main()
