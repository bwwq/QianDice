"""Reusable contract: scheduling, selective restore, retention and authenticated remote uploads."""
import base64
import hashlib
import hmac
import io
import json
from pathlib import Path
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

binary = Path(sys.argv[1]).resolve()
root = Path(tempfile.mkdtemp(prefix='千变 备份契约 '))
files, removed, violations = {}, [], []
dav_auth = 'Basic ' + base64.b64encode(b'backup-user:contract-password').decode()
access, secret, session = 'CONTRACTACCESS', 'contract-secret-key', 'contract-session-token'


class Remote(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def check(self, body=b''):
        if self.path.startswith('/bucket/'):
            payload = hashlib.sha256(body).hexdigest()
            assert self.headers['x-amz-content-sha256'] == payload, 'payload hash mismatch'
            assert self.headers['x-amz-security-token'] == session, 'session token missing'
            authorization = self.headers['Authorization']
            credential, fields = authorization.removeprefix('AWS4-HMAC-SHA256 Credential=').split(', SignedHeaders=')
            signed, signature = fields.split(', Signature=')
            key, date, region, service, terminator = credential.split('/')
            assert (key, region, service, terminator) == (access, 'us-east-1', 's3', 'aws4_request')
            names = signed.split(';')
            assert names == sorted(names) and 'host' in names
            headers = ''.join(f'{name}:{" ".join(self.headers[name].strip().split())}\n' for name in names)
            canonical = f'{self.command}\n{self.path}\n\n{headers}\n{signed}\n{payload}'
            scope = f'{date}/{region}/s3/aws4_request'
            to_sign = f'AWS4-HMAC-SHA256\n{self.headers["x-amz-date"]}\n{scope}\n{hashlib.sha256(canonical.encode()).hexdigest()}'
            signing = ('AWS4' + secret).encode()
            for value in [date, region, service, terminator]:
                signing = hmac.new(signing, value.encode(), hashlib.sha256).digest()
            assert hmac.compare_digest(signature, hmac.new(signing, to_sign.encode(), hashlib.sha256).hexdigest()), 'SigV4 mismatch'
        else:
            assert self.headers['Authorization'] == dav_auth, 'WebDAV Basic auth mismatch'

    def respond(self, status):
        self.send_response(status)
        self.send_header('Content-Length', '0')
        self.end_headers()

    def do_MKCOL(self):
        try:
            self.check()
            self.respond(405 if self.path == '/dav/' else 201)
        except Exception as e:
            violations.append(str(e)); self.respond(403)

    def do_PUT(self):
        body = self.rfile.read(int(self.headers['Content-Length']))
        try:
            self.check(body)
            if self.path.startswith('/failure/'):
                self.respond(503); return
            assert self.headers['Content-Type'] == 'application/zip'
            assert not self.path.endswith('.partial'), 'incomplete remote archive'
            with zipfile.ZipFile(io.BytesIO(body)) as archive:
                assert 'manifest.json' in archive.namelist()
            files[self.path] = body
            self.respond(200 if self.path.startswith('/bucket/') else 201)
        except Exception as e:
            violations.append(str(e)); self.respond(400)

    def do_DELETE(self):
        try:
            self.check()
            removed.append(self.path); files.pop(self.path, None)
            self.respond(204)
        except Exception as e:
            violations.append(str(e)); self.respond(403)


remote = ThreadingHTTPServer(('127.0.0.1', 0), Remote)
threading.Thread(target=remote.serve_forever, daemon=True).start()
remote_url = f'http://127.0.0.1:{remote.server_port}'
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0)); port = sock.getsockname()[1]
base = f'http://127.0.0.1:{port}/api/v1'
process, token = None, ''


def api(path, body=None, method=None):
    request = urllib.request.Request(base + path, data=json.dumps(body).encode() if body is not None else None,
        method=method or ('POST' if body is not None else 'GET'),
        headers={'Authorization': 'Bearer ' + token, 'x-qianbian': '1', 'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=15) as response:
        return json.load(response)


def start():
    global process, token
    process = subprocess.Popen([str(binary), '--root', str(root), '--listen', f'127.0.0.1:{port}'], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    for _ in range(100):
        try:
            token = (root / 'data/config/admin-token.txt').read_text().strip()
            api('/status'); return
        except (OSError, urllib.error.URLError):
            if process.poll() is not None:
                raise AssertionError(process.stderr.read().decode(errors='replace'))
            time.sleep(.05)
    raise AssertionError('backend did not start')


def stop():
    global process
    api('/shutdown', {})
    _, errors = process.communicate(timeout=15)
    assert process.returncode == 0, errors.decode(errors='replace')
    process = None


def command(text):
    return api('/simulate', {'context': {'platform': 'simulation', 'account': 'test', 'user': 'owner', 'group': 'group'}, 'command': text})


def force_due():
    # Move the persisted deadline while offline; no production-only short timer option.
    stop()
    state_path = root / 'data/backup-state.json'
    state = json.loads(state_path.read_text())
    state['next_at'] = 0
    state_path.write_text(json.dumps(state))
    start()
    for _ in range(100):
        state = api('/backups/status')
        if not state['running'] and state['next_at'] and state['next_at'] > time.time() and state['last_result'].get('id'):
            return state['last_result']
        time.sleep(.05)
    raise AssertionError('scheduled backup did not finish')


def selected():
    return {'game': True, 'config': False, 'plugins': False, 'rules': False, 'decks': False, 'logs': False}


try:
    start()
    command('.st new 调查员'); command('.st 力量55')
    plugin_file = root / 'data/plugins/user-note.txt'
    plugin_file.write_text('preserve this plugin file')
    with sqlite3.connect(root / 'data/qianbian.sqlite') as db:
        db.execute("INSERT INTO kv VALUES('plugin:fixture','counter','1',1)")
    config = api('/config')['backup']
    config['contents'] = selected()
    config['webdav'].update(enabled=True, url=remote_url + '/dav/千变 备份/', username='backup-user', password='contract-password')
    config['s3'].update(enabled=True, endpoint=remote_url, bucket='bucket', prefix='千变 备份', access_key=access, secret_key=secret, session_token=session)
    api('/config', {'backup': config}, 'PUT')
    public = api('/config')['backup']
    assert public['webdav']['password'] == '' and public['webdav']['has_password']
    assert public['s3']['secret_key'] == '' and public['s3']['session_token'] == ''
    api('/config', {'backup': public}, 'PUT')  # blank secrets preserve saved credentials
    result = api('/backups', {})
    assert result['status'] == 'success' and len(result['uploads']) == 2, result
    backup = result['id']
    snapshot = root / 'data/backups' / backup
    manifest = json.loads((snapshot / 'manifest.json').read_text())
    assert manifest['schema'] == 2 and manifest['contents'] == selected()
    assert not (snapshot / 'plugins').exists() and not (snapshot / 'config').exists()
    with sqlite3.connect(snapshot / 'qianbian.sqlite') as db:
        assert db.execute("SELECT count(*) FROM kv WHERE namespace<>'world'").fetchone()[0] == 0
    for archive_bytes in files.values():
        with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
            assert not any(name.startswith(('plugins/', 'config/')) for name in archive.namelist())
    assert all('%E5' in name and '%20' in name for name in files), list(files)
    command('.st 力量77')
    with sqlite3.connect(root / 'data/qianbian.sqlite') as db:
        db.execute("UPDATE kv SET value='2', revision=2 WHERE namespace='plugin:fixture'")
    stop()
    restored = subprocess.run([str(binary), '--root', str(root), 'restore', str(snapshot), '--confirm'], capture_output=True)
    assert restored.returncode == 0, restored.stderr
    start()
    assert command('.st 力量')['public'].endswith('55')
    assert plugin_file.read_text() == 'preserve this plugin file'
    with sqlite3.connect(root / 'data/qianbian.sqlite') as db:
        assert db.execute("SELECT value FROM kv WHERE namespace='plugin:fixture'").fetchone()[0] == '2'
    for patch in [{'interval_minutes': 0}, {'keep': 0}, {'contents': {key: False for key in selected()}}]:
        bad = api('/config')['backup']; bad.update(patch)
        try:
            api('/config', {'backup': bad}, 'PUT'); raise AssertionError('invalid backup config accepted')
        except urllib.error.HTTPError as e:
            assert e.code == 400
    config = api('/config')['backup']
    config.update(enabled=True, interval_minutes=1, keep=1)
    api('/config', {'backup': config}, 'PUT')
    manual = api('/backups', {})['id']
    first = force_due(); assert first['status'] == 'success', first
    second = force_due(); assert second['status'] == 'success' and second['id'] != first['id'], second
    names = api('/backups')
    assert first['id'] not in names and second['id'] in names and manual in names and backup in names
    assert len(removed) == 2 and all(first['id'] in name for name in removed), removed
    config = api('/config')['backup']
    config['webdav']['url'] = remote_url + '/failure/'
    config['s3']['enabled'] = False
    api('/config', {'backup': config}, 'PUT')
    failed = force_due()
    assert failed['status'] == 'partial' and failed['uploads'][0]['status'] == 'failed', failed
    assert (root / 'data/backups' / failed['id'] / 'manifest.json').exists()
    assert api('/backups/status')['next_at'] > time.time()
    assert not violations, violations
    stop()
    print('PASS: selective snapshot/restore, scheduled backup persistence, local/remote retention, WebDAV auth, S3 SigV4 and streaming ZIP, secret redaction, upload failure keeps local backup')
finally:
    if process is not None:
        process.terminate()
        process.communicate(timeout=15)
    remote.shutdown(); remote.server_close()
    shutil.rmtree(root)
