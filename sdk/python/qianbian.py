"""千变 JSON-RPC SDK。仅依赖 Python 标准库，stdout 专用于协议。"""
import json
import sys


def _write(packet):
    sys.stdout.write(json.dumps(packet, ensure_ascii=False) + '\n')
    sys.stdout.flush()


class Host:
    def __init__(self):
        self.sequence = 0

    def call(self, method, **params):
        self.sequence += 1
        request_id = f'host-{self.sequence}'
        _write({'jsonrpc': '2.0', 'id': request_id, 'method': method, 'params': params})
        response = json.loads(sys.stdin.readline())
        if response.get('id') != request_id:
            raise RuntimeError('Unexpected host response')
        if 'error' in response:
            raise RuntimeError(response['error']['message'])
        return response['result']


def serve(handler):
    host = Host()
    for line in sys.stdin:
        packet = json.loads(line)
        try:
            method = packet.get('method')
            if method == 'initialize':
                result = {'api': 1}
            elif method == 'health':
                result = {'ok': True}
            elif method == 'command':
                result = handler(packet.get('params', {}), host)
            else:
                raise ValueError(f'Unsupported method: {method}')
            response = {'jsonrpc': '2.0', 'id': packet['id'], 'result': result}
        except Exception as error:
            response = {'jsonrpc': '2.0', 'id': packet['id'], 'error': {'code': -32000, 'message': str(error)}}
        _write(response)
