"""Cross-build the CRT-free Windows export file owner (src/native/file-win32-x64.node).

Usage: python3 tools/build-file-win32.py
Runs on Linux with mingw-w64 (x86_64-w64-mingw32-gcc, x86_64-w64-mingw32-dlltool).
Node-API headers in tools/node-api come unmodified from node-v24.12.0-headers.
The import library binds Node-API symbols to the host node.exe, as node-gyp does.
The verification report is printed; set OPENVECTOR_BUILD_PROOF to also save it.
"""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
tools = root / 'tools'
source = tools / 'file-win32.c'
output = root / 'src/native/file-win32-x64.node'
output.parent.mkdir(exist_ok=True)
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
run = lambda *command: subprocess.run([str(part) for part in command], check=True, capture_output=True, text=True).stdout
build = Path(tempfile.mkdtemp(prefix='file-win32-'))

# Import only the Node-API functions the source calls.
symbols = sorted(set(re.findall(r'\b(napi_[a-z0-9_]+)\(', source.read_text())) - {'napi_register_module_v1'})
(build / 'node.def').write_text('LIBRARY node.exe\nEXPORTS\n' + ''.join(f'  {name}\n' for name in symbols))
run('x86_64-w64-mingw32-dlltool', '-d', build / 'node.def', '-l', build / 'node.lib')
command = ['x86_64-w64-mingw32-gcc', '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-fno-builtin',
           '-fno-stack-protector', '-fno-asynchronous-unwind-tables', '-DNAPI_VERSION=8', '-shared', '-nostdlib',
           '-Wl,-e,DllEntry', '-Wl,--no-insert-timestamp', '-s', source, build / 'node.lib', '-lkernel32', '-o', output]
run(*command)
# The module must import only kernel32 and node.exe: no C runtime DLL.
imports = sorted(set(re.findall(r'DLL Name: (\S+)', run('x86_64-w64-mingw32-objdump', '-p', output))))
if imports != ['KERNEL32.dll', 'node.exe']:
    raise RuntimeError(f'Unexpected DLL imports: {imports}')
proof = {'moduleSha256': digest(output), 'bytes': output.stat().st_size, 'sourceSha256': digest(source),
         'headersSha256': {path.name: digest(path) for path in sorted((tools / 'node-api').glob('*.h'))},
         'nodeApiImports': symbols, 'dllImports': imports, 'compiler': run('x86_64-w64-mingw32-gcc', '--version').splitlines()[0],
         'command': [str(part) if not str(part).startswith(str(build)) else Path(part).name for part in command]}
encoded = json.dumps(proof, indent=2) + '\n'
if proof_path := os.environ.get('OPENVECTOR_BUILD_PROOF'):
    Path(proof_path).write_text(encoded)
print(encoded, end='')
