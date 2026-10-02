"""Run the packaged desktop under a cloud-only virtual X display."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

root = Path(tempfile.mkdtemp(prefix='千变 桌面验收 '))
shutil.copytree(Path(sys.argv[1]), root, dirs_exist_ok=True)
base = 'http://127.0.0.1:9610/api/v1'
token = ''
ui = None

def api(path, body=None):
    request = urllib.request.Request(base + path, headers={'Authorization': 'Bearer ' + token, 'x-qianbian': '1', 'content-type': 'application/json'}, data=None if body is None else json.dumps(body).encode())
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.load(response)

def processes():
    result = []
    for directory in Path('/proc').iterdir():
        if not directory.name.isdigit():
            continue
        try:
            command = (directory / 'cmdline').read_bytes().replace(b'\0', b' ').decode()
            if str(root) not in command:
                continue
            data = (directory / 'status').read_text()
            rss = next((int(line.split()[1]) for line in data.splitlines() if line.startswith('VmRSS:')), 0)
            role = 'desktop' if 'qianbian_ui' in command else 'plugin' if 'worker' in command else 'core'
            result.append({'pid': int(directory.name), 'role': role, 'rssKiB': rss})
        except (OSError, UnicodeDecodeError):
            pass
    return result

try:
    ui = subprocess.Popen([str(root / 'qianbian-ui')], cwd=tempfile.gettempdir())
    for _ in range(60):
        if ui.poll() is not None:
            raise AssertionError('Desktop exited before startup')
        try:
            token = (root / 'data/config/admin-token.txt').read_text().strip()
            status = api('/status')
            break
        except (OSError, ValueError):
            time.sleep(.5)
    else:
        raise AssertionError('Desktop launcher did not start backend')
    assert os.path.samefile(status['data_directory'], root / 'data')
    time.sleep(4)
    assert ui.poll() is None, 'Flutter desktop did not remain running'
    running = processes()
    assert any(p['role'] == 'desktop' for p in running)
    Path('dist/linux-desktop-memory.json').write_text(json.dumps({'scenario': 'Ubuntu24.04 Xvfb desktop, five plugins, no QQ account', 'measurement': 'VmRSS KiB; shared pages counted per process', 'processes': running}, indent=2))
    ui.terminate()
    ui.wait(timeout=10)
    assert api('/status')['api'] == 1, 'Closing desktop stopped backend'
    api('/shutdown', {})
    for _ in range(50):
        if not processes():
            break
        time.sleep(.2)
    else:
        raise AssertionError('Backend shutdown left plugin processes alive')
    print('PASS: Linux desktop in Unicode portable directory, window/core lifetime, plugin cleanup')
finally:
    if ui and ui.poll() is None:
        ui.terminate()
        ui.wait(timeout=10)
    try:
        api('/shutdown', {})
    except Exception:
        pass
    for process in processes():
        try:
            os.kill(process['pid'], signal.SIGTERM)
        except ProcessLookupError:
            pass
