import {readFile, copyFile, cp, mkdir, readdir, writeFile} from 'node:fs/promises';
import {build} from 'esbuild';
import {join, relative, resolve, sep} from 'node:path';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {gzipSync} from 'node:zlib';
import {copyBundledNotices, root, rustNotices} from '../tools/native-notices.mjs';

const outdir = process.argv[2];
if (!outdir) throw new Error('Supply a new output directory.');
await mkdir(outdir);
const app = join(outdir, 'app');
await mkdir(app);
await cp('site', outdir, {recursive: true, filter: path => !path.split(sep).includes('.taste-skill')});
const licenses = join(outdir, 'licenses');
await mkdir(licenses);

// wasm-bindgen emits a static ES import for the module's WASI host functions.
const wasm = await readFile('generated/openvector_bg.wasm');
// Only content-addressed files receive immutable caching on both hosts.
const engine = 'engine/' + createHash('sha256').update(wasm).digest('hex') + '.wasm';
await mkdir(join(app, 'engine'));
const imports = WebAssembly.Module.imports(await WebAssembly.compile(wasm));
const names = imports.filter(entry => entry.module === 'wasi_snapshot_preview1').map(entry => entry.name);
await build({
  entryPoints: {openvector: 'launch.js', 'document-worker': 'document-worker.js'}, bundle: true, splitting: true, format: 'esm', minify: true, logOverride: {'duplicate-object-key': 'silent'}, outdir: app,
  define: {OPENVECTOR_ENGINE_PATH: JSON.stringify(engine)},
  plugins: [{
    name: 'wasi-browser-host',
    setup(builder) {
      builder.onResolve({filter: /^wasi$/}, () => ({path: resolve('threads.js')}));
      builder.onResolve({filter: /^wasi_snapshot_preview1$/}, () => ({path: 'wasi', namespace: 'host'}));
      builder.onLoad({filter: /.*/, namespace: 'host'}, () => ({
        contents: "import { wasi } from './browser-host.js';\n" + names.map(name =>
          `export const ${name} = (...args) => wasi.wasiImport.${name}(...args);`).join('\n'),
        resolveDir: resolve('.'),
      }));
    },
  }],
});
await writeFile(join(app, engine), wasm);
await writeFile(join(app, engine + '.gz'), gzipSync(wasm, {level: 9}));
await copyFile('index.html', join(app, 'index.html'));
await copyFile('_headers', join(outdir, '_headers'));
for (const name of await readdir(app)) {
  if (name.endsWith('.js')) {
    await writeFile(join(app, name + '.gz'), gzipSync(await readFile(join(app, name)), {level: 9}));
  }
}

const [artifacts, metadata] = JSON.parse(readFileSync(0, 'utf8'));
await writeFile(join(licenses, 'rust-licenses.txt'), rustNotices('wasm32-wasip1-threads', artifacts, metadata));
await writeFile(join(licenses, 'rust-version.txt'), execFileSync('rustc', ['-vV']));
copyBundledNotices(licenses);
const assets = join(root, 'rust/crates/openvector-gui/assets');
const rust = execFileSync('rustc', ['--print', 'sysroot'], {encoding: 'utf8'}).trim();
const docs = join(rust, 'share/doc/rust');
for (const [source, name] of [
  [join(root, 'LICENSE'), 'LICENSE.txt'],
  [join(assets, 'fonts/OFL.txt'), 'geist-fonts.txt'],
  [join(assets, 'icons/LICENSE'), 'lucide-icons.txt'],
  ['node_modules/@bjorn3/browser_wasi_shim/LICENSE-MIT', 'browser-wasi-shim.txt'],
  [join(docs, 'COPYRIGHT-library.html'), 'rust-std.html'],
  [join(root, 'tools/rust-std-licenses.json'), 'rust-std-supplement.json'],
]) await copyFile(source, join(licenses, name));
await cp(join(docs, 'licenses'), join(licenses, 'rust-toolchain'), {recursive: true});
await cp('wasi-licenses', join(licenses, 'wasi-sdk-25'), {recursive: true});

const buildFiles = ['Caddyfile', '_headers', 'README.md', 'package.json', 'package-lock.json', 'build-browser.py',
  'serve.py', 'check-worker.py', 'check-exports.py', 'bundle.mjs', 'index.html', 'site', 'launch.js', 'threads.js', 'browser-host.js', 'document-worker.js', 'wasi-licenses'];
execFileSync('tar', ['--exclude=web/site/.taste-skill', '-czf', join(licenses, 'openvector-source.tar.gz'), '-C', root,
  '.github/assets/openvector-logo.png', 'test/fixtures/browser-session-logo.jpeg', 'test/fixtures/browser-session-logo.png', 'docs/performance/wasm-prototype.json', 'LICENSE', 'src', 'rust/crates', 'rust/vendor', 'rust/Cargo.lock', 'rust/Cargo.toml',
  'tools/native-notices.mjs', 'tools/rust-std-licenses.json', 'tools/rust-upstream-licenses.json',
  ...buildFiles.map(file => 'web/' + file)]);

const entries = (await readdir(licenses, {recursive: true, withFileTypes: true})).filter(entry => entry.isFile());
const links = entries.map(entry => relative(licenses, join(entry.parentPath, entry.name)).split(sep).join('/')).sort();
await writeFile(join(licenses, 'index.html'), '<!doctype html><html lang="en"><meta charset="utf-8">' +
  '<title>OpenVector licenses and source</title><h1>OpenVector licenses and source</h1><ul>' +
  links.map(path => '<li><a href="' + path + '">' + path + '</a></li>').join('') + '</ul></html>');
