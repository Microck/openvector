"""Build the pinned IJG jpeg-6b default compressor without host imports."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
llvm = Path(sys.argv[1]).resolve()
vendor = root / 'src/vendor/jpeg-6b'
identity = json.loads((root / 'evidence/bitmap-jpeg-source-identity.json').read_text())
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
for filename, sha in identity['files'].items():
    assert digest(vendor / filename) == sha, filename
sources = [root / 'tools/bitmap-jpeg.c'] + [vendor / name for name in identity['files'] if name.endswith('.c')]
headers = list((root / 'tools/bitmap-jpeg-libc').glob('*.h'))
output = root / 'src/bitmap-jpeg.wasm'
command = [str(llvm / 'clang.exe'), '--target=wasm32', '-std=gnu99', '-O2', '-ffreestanding', '-fno-builtin',
    '-Wno-deprecated-non-prototype', '-DNO_GETENV', '-I' + str(vendor), '-I' + str(root / 'tools/bitmap-jpeg-libc'),
    '-nostdlib', '-Wl,--no-entry', '-Wl,--export-memory', '-Wl,--export=ov_reset', '-Wl,--export=malloc',
    '-Wl,--export=ov_jpeg_encode', '-Wl,--export=ov_jpeg_output', '-Wl,--export=ov_jpeg_error',
    '-Wl,--initial-memory=131072', '-Wl,--max-memory=2147483648', '-Wl,--strip-all',
    *map(str, sources), '-o', str(output)]
subprocess.run(command, check=True)
proof = {'wasmSha256': digest(output), 'bytes': output.stat().st_size,
    'sourceArchiveSha256': identity['archiveSha256'], 'upstreamFilesUnmodified': True,
    'sources': {str(path.relative_to(root)): digest(path) for path in sources + headers}, 'command': command}
(root / 'evidence/bitmap-jpeg-build-verification.json').write_text(json.dumps(proof, indent=2) + '\n')
print(json.dumps({'wasmSha256': proof['wasmSha256'], 'bytes': proof['bytes']}))
