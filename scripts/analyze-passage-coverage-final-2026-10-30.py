"""Offline full v6/v8 comparison on frozen source gold."""
import runpy
import sys
from pathlib import Path

if __name__ == '__main__':
    sys.argv.append('--passage-coverage-final')
    runpy.run_path(str(Path(__file__).with_name('analyze-extraction-selection-2026-10-30.py')), run_name='__main__')
