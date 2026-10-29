"""Build the freestanding FreeType coverage kernel with LLVM."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
llvm = Path(sys.argv[1]).resolve()
vendor = root / 'src/vendor/freetype-raster'
identity = json.loads((root / 'evidence/bitmap-raster-source-identity.json').read_text())
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
for name, sha in identity['vendoredSha256'].items():
    assert digest(vendor / name) == sha, name
output = root / 'src/bitmap-raster.wasm'
source = root / 'tools/bitmap-raster.c'
command = [str(llvm / 'clang.exe'), '--target=wasm32', '-std=gnu99', '-O2', '-ffreestanding', '-fno-builtin',
           '-I' + str(vendor), '-I' + str(root / 'tools/bitmap-raster-libc'), '-nostdlib',
           '-Wl,--no-entry', '-Wl,--export-memory', '-Wl,--export=__heap_base', '-Wl,--export=ov_coverage',
           '-Wl,--initial-memory=131072', '-Wl,--max-memory=2147483648', '-Wl,--strip-all', str(source), '-o', str(output)]
subprocess.run(command, check=True)
proof = {'wasmSha256': digest(output), 'bytes': output.stat().st_size, 'command': command,
         'wrapperSha256': digest(source), 'sourceIdentity': identity}
(root / 'evidence/bitmap-raster-build-verification.json').write_text(json.dumps(proof, indent=2) + '\n')
print(json.dumps({'wasmSha256': proof['wasmSha256'], 'bytes': proof['bytes']}))
