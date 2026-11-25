import init from './generated/openvector.js';
import {wasi} from './browser-host.js';

// WASI file descriptors belong to the tab. Forward file operations there so
// imports, exports and the editor all observe the same files and offsets.
self.onmessage = async ({data: {module, memory, thread, argument, reply}}) => {
  const mailbox = new Int32Array(reply);
  for (const name of Object.keys(wasi.wasiImport)) {
    if (!/^(fd_|path_)/.test(name)) continue;
    wasi.wasiImport[name] = (...args) => {
      Atomics.store(mailbox, 0, 0);
      self.postMessage({operation: 'wasi', name, args});
      Atomics.wait(mailbox, 0, 0);
      return Atomics.load(mailbox, 1);
    };
  }
  globalThis.openVectorHost.download = path => self.postMessage({operation: 'download', path});
  const exports = await init({module_or_path: module, memory});
  wasi.inst = {exports};
  exports.wasi_thread_start(thread, argument);
  self.close();
};
