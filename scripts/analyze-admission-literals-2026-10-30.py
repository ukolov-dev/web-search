"""Offline full v9/v10 comparison on source gold frozen before inference."""
import runpy
import sys
from pathlib import Path

if __name__ == '__main__':
    sys.argv.append('--admission-literals')
    runpy.run_path(str(Path(__file__).with_name('analyze-extraction-selection-2026-10-30.py')), run_name='__main__')
