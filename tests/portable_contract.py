"""Cloud-only reusable integration checks for persistence, auth and plugin lifecycle."""
import json
import os
from pathlib import Path
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import threading
import urllib.request
import urllib.error

binary = Path(sys.argv[1]).resolve()
root = Path(tempfile.mkdtemp(prefix='千变 portable '))
sock = socket.socket(); sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]; sock.close()
base = f'http://127.0.0.1:{port}/api/v1'
token = ''
process = None


def api(path, body=None, method=None, auth=True):
    headers = {'content-type': 'application/json', 'x-qianbian': '1'}
    if auth:
        headers['authorization'] = 'Bearer ' + token
    request = urllib.request.Request(base + path, data=None if body is None else json.dumps(body).encode(), headers=headers, method=method or ('POST' if body is not None else 'GET'))
    with urllib.request.urlopen(request, timeout=40) as response:
        return json.load(response)


def start():
    global process, token
    process = subprocess.Popen([str(binary), '--root', str(root), '--listen', f'127.0.0.1:{port}'], cwd=tempfile.gettempdir(), stdout=subprocess.DEVNULL)
    for _ in range(150):
        if process.poll() is not None:
            raise AssertionError('Backend exited before ready')
        try:
            token = (root / 'data/config/admin-token.txt').read_text().strip()
            api('/status')
            return
        except (FileNotFoundError, urllib.error.URLError):
            time.sleep(.1)
    raise AssertionError('Backend startup timeout')


def command(text):
    return api('/simulate', {'context': {'platform': 'qq', 'account': 'ignored', 'user': '调查员', 'group': '测试团', 'name': '调查员'}, 'text': text})


def stop():
    api('/shutdown', {})
    process.wait(timeout=35)


try:
    start()
    config_path = root / 'data/config/server.json'
    config = json.loads(config_path.read_text(encoding='utf-8'))
    config['listen'] = f'127.0.0.1:{port}'
    config_path.write_text(json.dumps(config), encoding='utf-8')
    readiness = subprocess.run([str(binary), '--root', str(root), 'ready'], capture_output=True, timeout=5)
    assert readiness.returncode == 0, 'launcher did not authenticate the running backend'
    token_path = root / 'data/config/admin-token.txt'
    token_path.write_text('wrong-token', encoding='utf-8')
    try:
        mismatch = subprocess.run([str(binary), '--root', str(root), 'ready'], capture_output=True, timeout=5)
        assert mismatch.returncode != 0, 'launcher accepted a different backend identity'
    finally:
        token_path.write_text(token, encoding='utf-8')
    api('/config', {'accounts': [{'id': 'contract', 'self_id': '1', 'mode': 'reverse', 'token': 'portable-contract-token-32-chars!', 'enabled': False}]}, 'PUT')
    masked = api('/config')
    assert masked['accounts'][0]['token'] == ''
    api('/config', {'accounts': masked['accounts']}, 'PUT')
    api('/config', {'cooldown_ms': 0}, 'PUT')
    assert api('/config')['accounts'][0]['id'] == 'contract', 'partial settings overwrote account configuration'
    try:
        api('/status', auth=False)
        raise AssertionError('Unauthenticated access accepted')
    except urllib.error.HTTPError as error:
        assert error.code == 401
    other = subprocess.run([str(binary), '--root', str(root)], capture_output=True, timeout=10)
    assert other.returncode != 0, 'second writer accepted'
    assert '创建' in command('.st new 调查员')['public']
    command('.st 理智60 力量50')
    command('.st lock')
    result = command('.sc 0/0 20')
    assert result['world']['players']['调查员']['cards']['调查员']['attrs']['理智'] == 60
    assert command('.rh 1d1')['private']
    assert '1' not in command('.rh 1d1')['public']
    try:
        command('.r 1/0')
    except urllib.error.HTTPError:
        pass
    assert '= 1' in command('.r 1d1')['public'], 'business error killed plugin'
    command('.st str+5')
    assert command('.st 力量')['public'].endswith('55'), 'alias relative edit failed'
    version = root / 'data/plugins/python-example/1.0.0'
    version.mkdir(parents=True)
    repo = Path(__file__).resolve().parents[1]
    for name in ['main.py', 'plugin.json']:
        shutil.copy(repo / 'examples/python-plugin' / name, version / name)
    shutil.copy(repo / 'sdk/python/qianbian.py', version / 'qianbian.py')
    manifest = json.loads((version / 'plugin.json').read_text())
    manifest['entry'] = sys.executable
    (version / 'plugin.json').write_text(json.dumps(manifest))
    api('/plugins/load', {'path': 'python-example/1.0.0/plugin.json'})
    assert '第 1 次' in command('.pyhello')['public']
    api('/plugins/load', {'path': 'python-example/1.0.0/plugin.json'})
    assert '第 2 次' in command('.pyhello')['public'], 'plugin storage lost during reload'
    with sqlite3.connect(root / 'data/qianbian.sqlite') as database:
        assert not database.execute("SELECT 1 FROM kv WHERE namespace = 'plugin:python-example'").fetchone(), 'simulation wrote production plugin storage'
    command('.pylater')
    for _ in range(15):
        if command('.pytimers')['public'] == '1':
            break
        time.sleep(.2)
    else:
        raise AssertionError('Persistent timer did not dispatch')
    api('/content/rules/contract.json', {'id': 'contract', 'label': '契约检定', 'faces': 2, 'comparison': 'lte'}, 'PUT')
    command('.st temp contract')
    assert '契约检定' in command('.ra 测试 2')['public']
    command('.st temp coc7')
    api('/plugins/python-example/disable', {})
    api('/plugins/decks/disable', {})
    backup = api('/backups', {})['id']
    assert (root / 'data/backups' / backup / 'manifest.json').exists()
    stop(); start()
    plugins = {p['manifest']['id']: p for p in api('/plugins')}
    assert not plugins['decks']['enabled']
    assert not plugins['python-example']['enabled']
    api('/plugins/decks/enable', {})
    api('/plugins/python-example/enable', {})
    assert '第 3 次' in command('.pyhello')['public']
    assert command('.st 力量')['public'].endswith('55')
    # An open SSE connection must not prevent graceful process shutdown.
    sse_done = threading.Event()
    def event_reader():
        request = urllib.request.Request(base + '/events', headers={'authorization': 'Bearer ' + token})
        try:
            with urllib.request.urlopen(request, timeout=40) as response:
                response.read()
        finally:
            sse_done.set()
    watcher = threading.Thread(target=event_reader, daemon=True)
    watcher.start()
    time.sleep(.2)
    stop()
    assert sse_done.wait(5), 'SSE stream did not close with backend'
    check = subprocess.run([str(binary), '--root', str(root), 'check-backup', str(root / 'data/backups' / backup)], capture_output=True)
    assert check.returncode == 0, check.stderr
    restore = subprocess.run([str(binary), '--root', str(root), 'restore', str(root / 'data/backups' / backup), '--confirm'], capture_output=True)
    assert restore.returncode == 0, restore.stderr
    start()
    assert command('.st 力量')['public'].endswith('55')
    stop()
    print('PASS: portable paths, auth, instance lock, card isolation, dark dice, plugin reload/disable/persistence, backup/restore')
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait(timeout=10)
    shutil.rmtree(root)
