// Shared by tools/build-native.mjs (npm) and tools/package-gui.mjs (GUI): runs the
// Cargo build and collects the license texts that must ship with its binaries.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, readdirSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../', import.meta.url));
export const workspace = join(root, 'rust');

export function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: workspace,
    encoding: 'utf8',
    stdio: ['inherit', 'pipe', 'inherit'],
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) throw new Error(`Cannot run ${command}: ${result.error.message}`);
  if (result.status !== 0) {
    throw new Error(`${command} failed (${result.signal ?? result.status}). Native artifacts were not packaged.`);
  }
  return result.stdout;
}

// Runs a locked `cargo build` for an explicit target and returns its compiler-artifact
// messages. Cargo reports fresh units too, so the list is complete on incremental builds.
export function cargoBuild(target, args) {
  return run('cargo', [
    'build', '--locked', '--target', target, ...args, '--message-format=json-render-diagnostics',
  ]).trim().split('\n').filter(Boolean).map((line) => JSON.parse(line))
    .filter((message) => message.reason === 'compiler-artifact');
}

// Notices for third-party code in the openvector crate, which every native binary
// links: the zlib and IJG jpeg-6b C sources that build.rs compiles, and the Rust
// translations of FreeType (bitmap_raster/gray_raster.rs), Qt under LGPL-2.1
// (bitmap_raster/square_stroke.rs), jpeg-js and IJG (jpeg_coefficients.rs,
// jpeg_input.rs), and UTIF.js and libtiff (tiff_fax.rs, tiff_legacy_codecs.rs and
// the floating predictor in tiff_input.rs). The JPEG and TIFF notices in src/
// describe both the JavaScript reference and the current Rust translations.
// The FreeType and Qt modification notes name the native Rust modules; the IJG
// wrapper notice distinguishes the native C wrapper from the WebAssembly one.
// These source notices and license texts are packaged at their existing paths,
// and each file is also copied unchanged beside the binaries. The Qt notes name
// the qt-bezier- prefix used here, so keep the two in step.
export function copyBundledNotices(destination) {
  const vendor = join(workspace, 'crates', 'openvector', 'vendor');
  for (const [source, filename] of [
    [join(vendor, 'zlib-1.2.3', 'README'), 'zlib-1.2.3.txt'],
    [join(vendor, 'jpeg-6b', 'README'), 'jpeg-6b.txt'],
    [join(vendor, 'jpeg-6b', 'openvector-notice.txt'), 'jpeg-6b-openvector-notice.txt'],
    [join(vendor, 'freetype-raster', 'FTL.TXT'), 'freetype-raster-FTL.txt'],
    [join(vendor, 'freetype-raster', 'modifications.txt'), 'freetype-raster-modifications.txt'],
    [join(vendor, 'qt-bezier', 'license-lgpl.txt'), 'qt-bezier-license-lgpl.txt'],
    [join(vendor, 'qt-bezier', 'lgpl-exception.txt'), 'qt-bezier-lgpl-exception.txt'],
    [join(vendor, 'qt-bezier', 'modifications.txt'), 'qt-bezier-modifications.txt'],
    [join(root, 'src', 'jpeg-licenses.txt'), 'jpeg-decoder-licenses.txt'],
    [join(root, 'src', 'tiff-licenses.txt'), 'tiff-decoder-licenses.txt'],
  ]) {
    copyFileSync(source, join(destination, filename));
  }
}

const licenseFile = /^(licen[cs]e|copying|copyright|notice|unlicense)/i;

// License texts of one package: the files it ships, or, for crates published without
// any, the upstream texts recorded in rust-upstream-licenses.json for that exact
// version. Each upstream entry is tied to the crate's published VCS commit.
function packageTexts(pkg, upstream) {
  const directory = dirname(pkg.manifest_path);
  const files = readdirSync(directory)
    .filter((name) => licenseFile.test(name) && statSync(join(directory, name)).isFile());
  if (pkg.license_file && !files.includes(pkg.license_file)) files.push(pkg.license_file);
  if (files.length) {
    return files.sort().map((file) => ({ origin: file, text: readFileSync(join(directory, file), 'utf8') }));
  }
  const entry = upstream[`${pkg.name}@${pkg.version}`];
  if (!entry) return undefined;
  const vcs = join(directory, '.cargo_vcs_info.json');
  // Cargo records the locked Git revision in source; registry archives record
  // their publish revision in .cargo_vcs_info.json.
  const commit = pkg.source?.startsWith('git+')
    ? new URL(pkg.source.slice(4)).hash.slice(1)
    : existsSync(vcs) ? JSON.parse(readFileSync(vcs, 'utf8')).git?.sha1 : undefined;
  if (entry.commit !== commit) {
    throw new Error(`${pkg.name} ${pkg.version} resolves to commit ${commit}, but tools/rust-upstream-licenses.json records ${entry.commit}.`);
  }
  return Object.entries(entry.files).map(([origin, text]) => ({ origin, text }));
}

// Builds the Rust third-party license document for one Cargo build. `artifacts` are
// the compiler-artifact messages of that build, so the package set is exactly what
// Cargo compiled. Packages compiled for `target` are "linked" into the binaries;
// build scripts, procedural macros and their dependencies compile for the host and
// are listed as "build". Workspace crates are OpenVector's own MIT code.
export function rustNotices(target, artifacts) {
  const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--format-version', '1']));
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const members = new Set(metadata.workspace_members);
  const roles = new Map();
  for (const artifact of artifacts) {
    if (members.has(artifact.package_id)) continue;
    // With an explicit --target, Cargo writes target units under <target-dir>/<triple>/
    // and host units directly under <target-dir>/<profile>/.
    const linked = artifact.filenames.some((filename) =>
      relative(metadata.target_directory, filename).split(sep)[0] === target);
    if (linked || !roles.has(artifact.package_id)) roles.set(artifact.package_id, linked ? 'linked' : 'build');
  }

  const upstream = JSON.parse(readFileSync(join(root, 'tools', 'rust-upstream-licenses.json'), 'utf8'));
  const listed = [...roles].map(([id, role]) => ({ pkg: packages.get(id), role }))
    .sort((a, b) => a.pkg.name.localeCompare(b.pkg.name) || a.pkg.version.localeCompare(b.pkg.version));
  const texts = new Map();
  const missing = [];
  for (const { pkg } of listed) {
    const found = packageTexts(pkg, upstream);
    if (!found) {
      missing.push(`${pkg.name} ${pkg.version} (${pkg.license ?? 'no license field'}, ${pkg.repository ?? pkg.source})`);
      continue;
    }
    for (const { origin, text } of found) addText(texts, text, `${pkg.name} ${pkg.version}: ${origin}`);
  }
  if (missing.length) {
    throw new Error(`No license text found for these Rust packages:\n  ${missing.join('\n  ')}\nAdd the upstream texts for their published commits to tools/rust-upstream-licenses.json.`);
  }

  const notes = listed.flatMap(({ pkg }) => {
    const note = upstream[`${pkg.name}@${pkg.version}`]?.note;
    return note ? [`${pkg.name} ${pkg.version}: ${note}`] : [];
  });
  return [
    `Third-party Rust packages compiled for ${target}`,
    '',
    'Generated from the Cargo build that produced these binaries and the license',
    'files in each package source. "linked" packages are compiled into the',
    'binaries; "build" packages ran only at build time (build scripts, procedural',
    'macros). Packages published without a license file use the text from their',
    'upstream repository at the linked URL.',
    '',
    ...listed.map(({ pkg, role }) => `${pkg.name} ${pkg.version} | ${role} | ${pkg.license ?? `license file ${pkg.license_file}`} | ${pkg.source}`),
    ...(notes.length ? ['', ...notes] : []),
    ...sections(texts),
    '',
  ].join('\n');
}

// Records that `user` is covered by license `text`, so each text is printed once.
function addText(texts, text, user) {
  const users = texts.get(text) ?? [];
  if (!users.includes(user)) users.push(user);
  texts.set(text, users);
}

// Each license text once, under the names of everything it covers.
function sections(texts) {
  return [...texts].flatMap(([text, users]) => ['', '='.repeat(78), ...users, '-'.repeat(78), text.trimEnd()]);
}

// Static C runtime files that the GNU linker driver and rustc's target link arguments
// add to every executable and shared library. Linker traces of the OpenVector binaries
// confirmed these inputs (evidence/toolchain-license-notices-verification.json). The
// system libraries they link dynamically (glibc, libgcc_s, Windows DLLs) do not ship.
// Windows import libraries for other DLLs come from the same package as libkernel32.a.
const runtimes = [
  {
    target: /-unknown-linux-gnu$/,
    linker: () => 'cc',
    files: ['Scrt1.o', 'crti.o', 'crtn.o', 'crtbeginS.o', 'crtendS.o', 'libgcc.a', 'libc_nonshared.a'],
  },
  {
    // Retained ARM64 Darwin CLI/addon linker traces contain no static C runtime
    // objects. SDK .tbd files describe dynamic system libraries, which do not ship.
    target: /^aarch64-apple-darwin$/,
    linker: () => 'cc',
    files: [],
  },
  {
    target: /-pc-windows-gnu$/,
    linker: (target) => `${target.split('-')[0]}-w64-mingw32-gcc`,
    files: ['crt2.o', 'dllcrt2.o', 'crtbegin.o', 'crtend.o', 'libmingw32.a', 'libmingwex.a', 'libmsvcrt.a',
      'libpthread.a', 'libkernel32.a', 'libgcc.a', 'libgcc_eh.a'],
  },
];

// Notices for the toolchain code in every binary built for `target`: the Rust standard
// library and the C runtime files its linker adds. Returns the text of
// toolchain-licenses.txt and the path of the standard library's own copyright notice,
// which ships unchanged as rust-std-COPYRIGHT-library.html. Call it before compiling
// so an unsupported toolchain fails before any build work.
//
// The linker is the one rustc uses: CARGO_TARGET_<TRIPLE>_LINKER or the target's
// default. A linker set only in a Cargo config file is not read. On GNU targets,
// runtime files are attributed to their Debian packages, which needs dpkg-query.
export function toolchainNotices(target) {
  const runtime = runtimes.find((entry) => entry.target.test(target));
  if (!runtime) {
    throw new Error(`No C runtime notices are defined for ${target}. Trace its linker inputs and add them to tools/native-notices.mjs.`);
  }
  const texts = new Map();

  // Each toolchain ships the standard library's notice with the license texts of the
  // out-of-tree crates it lists. Its in-tree files only name
  // SPDX licenses, whose texts sit beside it. The C and assembly objects in the
  // compiler_builtins rlib come from LLVM compiler-rt (debug source paths
  // lib/builtins/*), which that notice omits; the whole-project COPYRIGHT.html
  // covers it through its src/llvm-project entry.
  const version = run('rustc', ['-vV']);
  const field = (name) => version.split('\n').find((line) => line.startsWith(`${name}: `))?.slice(name.length + 2);
  const sysroot = run('rustc', ['--print', 'sysroot']).trim();
  const docs = join(sysroot, 'share', 'doc', 'rust');
  const fields = (html) => [...html.matchAll(/<p><b>(License|Copyright):<\/b> ([^<]*)<\/p>/g)]
    .map(([, name, value]) => ({ name, value }));
  const stdNotice = join(docs, 'COPYRIGHT-library.html');
  const html = readFileSync(stdNotice, 'utf8');
  const inTree = fields(html.slice(html.indexOf('id="in-tree-files"'), html.indexOf('id="out-of-tree-dependencies"')));
  const projectNotice = join(docs, 'COPYRIGHT.html');
  const project = readFileSync(projectNotice, 'utf8');
  const llvmStart = project.indexOf('<code>src/llvm-project</code>');
  const llvm = llvmStart < 0 ? [] : fields(project.slice(llvmStart, project.indexOf('</div>', llvmStart)));
  if (!inTree.length || !llvm.some(({ name }) => name === 'License')) {
    throw new Error(`Cannot read the in-tree licenses of ${stdNotice} or the src/llvm-project entry of ${projectNotice}.`);
  }
  const ids = new Set([...inTree, ...llvm].filter(({ name }) => name === 'License')
    .flatMap(({ value }) => value.split(/[\s()]+/))
    .filter((id) => id && !['AND', 'OR', 'WITH'].includes(id)));
  for (const id of [...ids].sort()) {
    const path = join(docs, 'licenses', `${id}.txt`);
    addText(texts, readFileSync(path, 'utf8'), `Rust toolchain: ${path}`);
  }

  // That notice can lag the crates std was compiled from: 1.99.1 omits addr2line,
  // adler2, miniz_oxide and object, and lists memchr 2.8.0 instead of 2.8.3. The rmeta
  // of each std crate for the target names the crate sources it was built from
  // (/rust/deps/<crate>-<version>). Each one the notice lacks needs its published
  // license files in tools/rust-std-licenses.json.
  const stdLib = join(sysroot, 'lib', 'rustlib', target, 'lib');
  const sources = new Set(readdirSync(stdLib).filter((name) => name.endsWith('.rmeta')).flatMap((name) =>
    [...readFileSync(join(stdLib, name), 'latin1').matchAll(/\/rust\/deps[\\/]([\w.+-]+)/g)].map((match) => match[1])));
  const stdCrates = JSON.parse(readFileSync(join(root, 'tools', 'rust-std-licenses.json'), 'utf8'));
  const omitted = [...sources].filter((source) => !html.includes(`📦 ${source}`)).sort().map((source) => {
    const [, name, crateVersion] = source.match(/^(.+)-(\d[^-]*)$/) ?? [, source, 'without version'];
    const entry = stdCrates[`${name}@${crateVersion}`];
    if (!entry) {
      throw new Error(`${stdNotice} omits ${name} ${crateVersion}, which the standard library for ${target} was compiled from. Add its published license files to tools/rust-std-licenses.json.`);
    }
    for (const [file, text] of Object.entries(entry.files)) addText(texts, text, `${name} ${crateVersion}: ${file}`);
    return `${name} ${crateVersion} | ${entry.license} | ${entry.crate} (sha256 ${entry.checksum})`;
  });

  const linker = process.env[`CARGO_TARGET_${target.toUpperCase().replaceAll('-', '_')}_LINKER`] || runtime.linker(target);
  const packages = new Map();
  const files = runtime.files.map((file) => {
    const found = run(linker, [`-print-file-name=${file}`]).trim();
    if (!isAbsolute(found)) throw new Error(`${linker} cannot find ${file}, which it links into every ${target} binary.`);
    const path = realpathSync(found);
    const owner = run('dpkg-query', ['-S', path]).split('\n')[0].split(': ')[0];
    if (!packages.has(owner)) packages.set(owner, run('dpkg-query', ['-W', '-f=${Version}', owner]).trim());
    return `${file} | ${path} | ${owner} ${packages.get(owner)}`;
  });
  for (const [owner, packageVersion] of packages) {
    const name = owner.split(':')[0];
    const copyright = `/usr/share/doc/${name}/copyright`;
    const text = readFileSync(copyright, 'utf8');
    addText(texts, text, `${name} ${packageVersion}: ${copyright}`);
    for (const [path] of text.matchAll(/\/usr\/share\/common-licenses\/[\w+-]+(\.\d+)*/g)) {
      addText(texts, readFileSync(path, 'utf8'), `${name} ${packageVersion}: ${path}`);
    }
  }

  return {
    stdNotice,
    text: [
      `Toolchain code compiled into the binaries for ${target}`,
      '',
      `Rust standard library from rustc ${field('release')} (${field('commit-hash')}).`,
      'rust-std-COPYRIGHT-library.html is the copyright notice that release ships for',
      'the standard library and the crates it uses, copied unchanged from',
      `${stdNotice}.`,
      'Its compiler_builtins crate also contains objects built from LLVM compiler-rt,',
      'which the Rust source tree carries in src/llvm-project. For that directory,',
      `${projectNotice} lists:`,
      ...llvm.map(({ name, value }) => `  ${name}: ${value}`),
      'The license texts named for in-tree standard library files and for',
      'src/llvm-project follow below.',
      ...(omitted.length ? [
        '',
        'That notice omits these crates the standard library was compiled from. Their',
        'license files, from the published crates whose checksums the release lists in',
        'library/Cargo.lock, follow below:',
        ...omitted,
      ] : []),
      '',
      ...(files.length ? [
        `C runtime files that ${linker} links into every binary:`,
        ...files,
        '',
        'The Debian copyright file of each package follows below, with the license',
        'texts it references from /usr/share/common-licenses.',
      ] : [
        `${linker} links no static C runtime files into ${target} binaries; the system`,
        'libraries they link dynamically do not ship.',
      ]),
      ...sections(texts),
      '',
    ].join('\n'),
  };
}
