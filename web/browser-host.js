import {ConsoleStdout, Directory, File as WasiFile, OpenFile, PreopenDirectory, WASI} from '@bjorn3/browser_wasi_shim';

const inputs = new Directory([]);
const downloads = new Directory([]);
const settings = new Directory([]);
const root = new PreopenDirectory('/', new Map([['images', inputs], ['downloads', downloads], ['settings', settings], ['tmp', new Directory([])]]));
export const wasi = new WASI(['openvector'], ['HOME=/', 'TMPDIR=/tmp', 'PWD=/'], [
  new OpenFile(new WasiFile([])),
  ConsoleStdout.lineBuffered(line => console.info(line)),
  ConsoleStdout.lineBuffered(line => console.error(line)),
  root,
], {debug: false});

let nextSource = 0;
export async function importImages(files) {
  const directory = String(++nextSource);
  const entries = new Directory([]);
  const paths = [];
  for (const file of files) {
    const name = file.name.replaceAll('/', '_');
    entries.contents.set(name, new WasiFile(await file.arrayBuffer()));
    paths.push('/images/' + directory + '/' + name);
  }
  inputs.contents.set(directory, entries);
  return paths;
}

globalThis.openVectorHost = {
  pickImages(multiple) {
    return new Promise((resolve, reject) => {
      const input = document.createElement('input');
      input.type = 'file';
      input.multiple = multiple;
      input.accept = '.png,.jpg,.jpeg,.bmp,.gif,.tif,.tiff';
      input.onchange = () => importImages(input.files).then(resolve, reject);
      input.oncancel = () => resolve([]);
      input.click();
    });
  },
  download(path) {
    const name = path.split('/').pop();
    const {inode_obj: file} = root.path_lookup(path.replace(/^\/+/, ''), 0);
    if (!(file instanceof WasiFile)) throw new Error('Cannot read exported file ' + name);
    const url = URL.createObjectURL(new Blob([file.data]));
    const link = document.createElement('a');
    link.href = url;
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 60000);
  },
};
