"""Exercise the published native Rust SDK as an independent JSON-RPC process."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time

started = time.perf_counter()
worker = subprocess.Popen([sys.argv[1]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8')
selector = selectors.DefaultSelector()
selector.register(worker.stdout, selectors.EVENT_READ)
trace = []

def send(packet):
    trace.append({'direction': 'to_plugin', 'payload': packet})
    worker.stdin.write(json.dumps(packet, ensure_ascii=False) + '\n')
    worker.stdin.flush()

def receive():
    assert selector.select(5), 'Rust SDK response timeout'
    value = json.loads(worker.stdout.readline())
    trace.append({'direction': 'from_plugin', 'payload': value})
    return value

try:
    send({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'api': 1, 'preflight': True}})
    assert receive()['result']['api'] == 1
    for request_id, failed in [(2, False), (3, True), (4, False)]:
        send({'jsonrpc': '2.0', 'id': request_id, 'method': 'command', 'params': {'context': {'user': '11001'}, 'command': 'rusthello'}})
        host = receive()
        assert host['method'] == 'dice.roll' and host['params']['expression'] == '1d20'
        reply = {'jsonrpc': '2.0', 'id': host['id']}
        reply['error' if failed else 'result'] = {'code': -32000, 'message': '模拟宿主错误'} if failed else {'total': 7}
        send(reply)
        response = receive()
        assert response['id'] == request_id
        assert '模拟宿主错误' in response['error']['message'] if failed else '投出了 7' in response['result']['public']
    worker.stdin.close()
    assert worker.wait(timeout=5) == 0, 'Rust SDK did not stop on EOF'
    report = Path(os.environ.get('QIANBIAN_TEST_REPORT_DIR', 'dist/functional-report'))
    report.mkdir(parents=True, exist_ok=True)
    (report / 'rust_sdk.json').write_text(json.dumps({'id': 'rust_sdk', 'status': 'passed', 'elapsed_ms': round((time.perf_counter() - started) * 1000, 2), 'features': ['initialize', 'reverse host callback', 'JSON-RPC success/error', 'worker survives business error', 'EOF cleanup']}, ensure_ascii=False, indent=2), encoding='utf-8')
    (report / 'transcripts').mkdir(exist_ok=True)
    (report / 'transcripts/rust_sdk.json').write_text(json.dumps(trace, ensure_ascii=False, indent=2), encoding='utf-8')
    print('PASS: native Rust SDK initialization, host calls, errors, EOF')
finally:
    selector.close()
    if worker.poll() is None:
        worker.kill()
        worker.wait(timeout=5)
