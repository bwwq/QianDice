"""Cloud acceptance: portable extraction, desktop/core lifetime and measured RSS."""
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

root = Path(tempfile.mkdtemp(prefix='千变 桌面验收 '))
entry = root / 'qianbian-ui.exe'
shutil.copyfile(Path(sys.argv[1]).resolve(), entry)
os.environ['QIANBIAN_CHECK_ROOT'] = str(root)
base = 'http://127.0.0.1:9610/api/v1'
token = ''

def api(path, body=None):
    request = urllib.request.Request(base + path, headers={'Authorization': 'Bearer ' + token, 'x-qianbian': '1', 'content-type': 'application/json'}, data=None if body is None else json.dumps(body).encode())
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.load(response)

def processes():
    command = "$p = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($env:QIANBIAN_CHECK_ROOT, [StringComparison]::OrdinalIgnoreCase) } | Select-Object ProcessId, Name, WorkingSetSize); ConvertTo-Json -InputObject $p -Compress"
    result = subprocess.run(['powershell', '-NoProfile', '-Command', command], capture_output=True, text=True, check=True, timeout=20)
    return json.loads(result.stdout)

try:
    subprocess.run([str(entry)], cwd=tempfile.gettempdir(), timeout=90, check=True)
    for _ in range(90):
        try:
            token = (root / 'data/config/admin-token.txt').read_text(encoding='utf-8').strip()
            status = api('/status')
            break
        except (OSError, ValueError):
            time.sleep(1)
    else:
        raise AssertionError('Self-extracting launcher did not start backend')
    assert os.path.samefile(status['data_directory'], root / 'data')
    time.sleep(3)
    running = processes()
    windows = [p for p in running if p['Name'] == 'qianbian_ui.exe']
    assert windows, 'Flutter desktop exited before acceptance'
    measurement = {'platform': platform.platform(), 'scenario': 'idle, five builtin workers, no QQ account', 'processes': running, 'units': 'bytes; Windows WorkingSetSize, shared pages counted per process'}
    Path('dist/windows-memory.json').write_text(json.dumps(measurement, indent=2), encoding='utf-8')
    for process in windows:
        subprocess.run(['taskkill', '/PID', str(process['ProcessId']), '/F'], check=True, capture_output=True)
    assert api('/status')['api'] == 1, 'Closing desktop stopped backend'
    api('/shutdown', {})
    for _ in range(20):
        if not processes():
            break
        time.sleep(1)
    else:
        raise AssertionError('Backend shutdown left plugin/desktop processes alive')
    print('PASS: Unicode portable extraction, authenticated backend, desktop lifetime, plugin cleanup')
finally:
    try:
        api('/shutdown', {})
    except Exception:
        pass
    # Only terminate processes inside this freshly-created acceptance directory.
    for process in processes():
        subprocess.run(['taskkill', '/PID', str(process['ProcessId']), '/T', '/F'], capture_output=True)
