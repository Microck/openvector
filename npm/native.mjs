import { accessSync, constants } from 'node:fs';
import { fileURLToPath } from 'node:url';

export function nativePath(filename, executable = false) {
  const platform = `${process.platform}-${process.arch}`;
  const path = fileURLToPath(new URL(`../native/${platform}/${filename}`, import.meta.url));
  try {
    accessSync(path, executable && process.platform !== 'win32' ? constants.X_OK : constants.F_OK);
  } catch (cause) {
    throw new Error(
      `OpenVector cannot access its native ${platform} artifact: ${path}. ` +
      'From a source checkout, run npm run build with Rust/Cargo and a C compiler installed. ' +
      'For an installed package, install an archive built for your platform and architecture. ' +
      'There is no JavaScript engine fallback.',
      { cause },
    );
  }
  return path;
}
