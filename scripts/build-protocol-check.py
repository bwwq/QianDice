"""Cloud packaging only: prepare a standalone OneBot verifier for subsequent runs."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

build = subprocess.run(['cargo', 'test', '--locked', '--release', '--test', 'onebot_contract', '--no-run', '--message-format=json'], capture_output=True, text=True)
print(build.stderr)
if build.returncode:
    print(build.stdout)
    sys.exit(build.returncode)
for line in build.stdout.splitlines():
    try:
        packet = json.loads(line)
    except ValueError:
        continue
    if packet.get('reason') == 'compiler-artifact' and packet.get('target', {}).get('name') == 'onebot_contract' and packet.get('executable'):
        Path('dist').mkdir(exist_ok=True)
        shutil.copyfile(packet['executable'], 'dist/qianbian-protocol-check')
        Path('dist/qianbian-protocol-check').chmod(0o755)
        print('Prepared dist/qianbian-protocol-check; future verification needs no Cargo or Rust.')
        break
else:
    sys.exit('Protocol verifier executable not found')
