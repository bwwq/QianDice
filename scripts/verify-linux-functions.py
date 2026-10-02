"""Compile once, then time concurrent OneBot contract execution separately."""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

report = Path('dist/functional-report').resolve()
report.mkdir(parents=True, exist_ok=True)
(report / 'transcripts').mkdir(exist_ok=True)
os.environ['QIANBIAN_TEST_REPORT_DIR'] = str(report)
compile_started = time.perf_counter()
build = subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'onebot_contract', '--no-run', '--message-format=json'], capture_output=True, text=True)
if build.returncode:
    print(build.stdout)
    print(build.stderr)
    (report / 'onebot-tests.log').write_text(build.stdout + build.stderr, encoding='utf-8')
    (report / 'summary.json').write_text(json.dumps({'status':'failed','phase':'compilation','cases':[]}), encoding='utf-8')
    sys.exit(build.returncode)
binary = None
for line in build.stdout.splitlines():
    try:
        packet = json.loads(line)
    except ValueError:
        continue
    if packet.get('reason') == 'compiler-artifact' and packet.get('target', {}).get('name') == 'onebot_contract' and packet.get('executable'):
        binary = packet['executable']
assert binary, 'Compiled OneBot contract executable not found'
compile_seconds = time.perf_counter() - compile_started
started = time.perf_counter()
result = subprocess.run([binary, '--nocapture', '--test-threads=4'], capture_output=True, text=True)
runtime_seconds = time.perf_counter() - started
print(result.stdout)
print(result.stderr)
(report / 'onebot-tests.log').write_text(result.stdout + result.stderr, encoding='utf-8')
summary = {'platform': platform.platform(), 'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), 'protocol': 'OneBot 11 official specification, numeric IDs, universal/forward WebSocket', 'parallel_test_groups': 4, 'compilation_seconds': round(compile_seconds, 3), 'execution_seconds': round(runtime_seconds, 3), 'status': 'passed' if result.returncode == 0 else 'failed', 'cases': []}
for path in sorted(report.glob('*.json')):
    if path.name == 'summary.json':
        continue
    summary['cases'].append(json.loads(path.read_text(encoding='utf-8')))
(report / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
print(f'Concurrent OneBot test execution: {runtime_seconds:.3f}s; compilation: {compile_seconds:.3f}s')
sys.exit(result.returncode)
