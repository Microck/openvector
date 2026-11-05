"""Build the canonical zlib 1.2.3 filtered PNG compressor with LLVM."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
llvm = Path(sys.argv[1]).resolve()
vendor = root / 'src/vendor/zlib-1.2.3'
identity = json.loads((vendor / 'source-identity.json').read_text())
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
for filename, expected in identity['files'].items():
    assert digest(vendor / filename) == expected, filename
output = root / 'src/bitmap-deflate.wasm'
sources = [root / 'tools/bitmap-deflate.c'] + [vendor / filename for filename in ['adler32.c', 'deflate.c', 'trees.c', 'zutil.c']]
headers = [root / 'tools/bitmap-deflate-libc' / name for name in ['stdlib.h', 'string.h']]
# Upstream 1.2.3 uses valid K&R definitions; keep those sources unchanged.
command = [str(llvm / 'clang.exe'), '--target=wasm32', '-std=gnu99', '-Wno-deprecated-non-prototype',
           '-O2', '-ffreestanding', '-fno-builtin', '-DNO_GZIP', '-DNO_ERRNO_H',
           '-I' + str(vendor), '-I' + str(root / 'tools/bitmap-deflate-libc'), '-nostdlib',
           '-Wl,--no-entry', '-Wl,--export-memory', '-Wl,--export=ov_reset', '-Wl,--export=malloc',
           '-Wl,--export=ov_deflate', '-Wl,--initial-memory=131072', '-Wl,--max-memory=2147483648',
           '-Wl,--strip-all', *map(str, sources), '-o', str(output)]
subprocess.run(command, check=True)
proof = {'wasmSha256': digest(output), 'bytes': output.stat().st_size,
         'sourceArchiveSha256': identity['archiveSha256'], 'upstreamFilesUnmodified': True,
         'sources': {str(path.relative_to(root)): digest(path) for path in sources + headers}, 'command': command}
(root / 'evidence').mkdir(exist_ok=True)
(root / 'evidence/bitmap-deflate-build-verification.json').write_text(json.dumps(proof, indent=2) + '\n')
print(json.dumps({'wasmSha256': proof['wasmSha256'], 'bytes': proof['bytes']}))
