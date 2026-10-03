"""Run concurrent OneBot verification against an existing executable; never compile."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--backend', default='dist/qianbian-linux-x86_64')
parser.add_argument('--runner', default='dist/qianbian-protocol-check')
parser.add_argument('--report-dir', default='dist/functional-report')
args = parser.parse_args()
backend, runner = Path(args.backend).resolve(), Path(args.runner).resolve()
for path in [backend, runner]:
    if not path.is_file():
        parser.error(f'Executable not found: {path}. Obtain the prepared Linux executables first.')
report = Path(args.report_dir).resolve()
report.mkdir(parents=True, exist_ok=True)
(report / 'transcripts').mkdir(exist_ok=True)
os.environ['QIANBIAN_TEST_REPORT_DIR'] = str(report)
os.environ['QIANBIAN_TEST_BINARY'] = str(backend)
started = time.perf_counter()
started_wall = time.time()
result = subprocess.run([str(runner), '--nocapture', '--test-threads=4'], capture_output=True, text=True)
runtime_seconds = time.perf_counter() - started
print(result.stdout)
print(result.stderr)
(report / 'onebot-tests.log').write_text(result.stdout + result.stderr, encoding='utf-8')
revision = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
summary = {
    'platform': platform.platform(),
    'commit': revision.stdout.strip() if revision.returncode == 0 else None,
    'backend': {'path': str(backend), 'sha256': hashlib.sha256(backend.read_bytes()).hexdigest()},
    'test_runner': {'path': str(runner), 'sha256': hashlib.sha256(runner.read_bytes()).hexdigest()},
    'protocol': 'OneBot 11 official specification; actual forward/universal WebSocket frames',
    'parallel_test_groups': 4, 'compilation_seconds': 0,
    'execution_seconds': round(runtime_seconds, 3),
    'status': 'passed' if result.returncode == 0 else 'failed', 'cases': [],
    'silence_observation_ms': 500,
}
# Do not reuse successful case records from a previous execution.
for name in ['rules_cards', 'decks_logs_permissions', 'forward_echo', 'plugin_failure']:
    path = report / f'{name}.json'
    if path.exists() and path.stat().st_mtime >= started_wall:
        summary['cases'].append(json.loads(path.read_text(encoding='utf-8')))
(report / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
print(f'Concurrent OneBot execution against existing binary: {runtime_seconds:.3f}s; no compilation')
sys.exit(result.returncode)
