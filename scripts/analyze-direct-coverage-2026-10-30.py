"""Run the shared paired analyzer with the v5/v6 direct-coverage configuration."""
import runpy
import sys
from pathlib import Path

if __name__ == '__main__':
    sys.argv.append('--direct-coverage')
    runpy.run_path(str(Path(__file__).with_name('analyze-extraction-selection-2026-10-30.py')), run_name='__main__')
