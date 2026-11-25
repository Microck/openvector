// The WASI threads ABI returns an ID synchronously. The guest initializes
// the new pthread's stack and thread-local storage in wasi_thread_start.
import {wasi as wasiConstants} from '@bjorn3/browser_wasi_shim';
import {wasi} from './browser-host.js';

let runtime;
let nextThread = 1;
export function setThreadRuntime(module, memory) {
  runtime = {module, memory};
}
function spawn(argument) {
  const thread = nextThread++;
  const worker = new Worker(new URL('document-worker.js', import.meta.url), {type: 'module'});
  const reply = new SharedArrayBuffer(8);
  const mailbox = new Int32Array(reply);
  worker.onerror = event => {
    console.error('Document worker stopped', event.message);
    const dialog = document.createElement('dialog');
    const reload = document.createElement('button');
    reload.textContent = 'Reload OpenVector';
    reload.onclick = () => location.reload();
    dialog.append('The document worker stopped. ', reload);
    document.body.append(dialog);
    dialog.showModal();
  };
  worker.onmessage = ({data}) => {
    if (data.operation === 'download') {
      globalThis.openVectorHost.download(data.path);
    } else if (data.operation === 'wasi') {
      let errno = wasiConstants.ERRNO_IO;
      try {
        errno = wasi.wasiImport[data.name](...data.args);
      } finally {
        Atomics.store(mailbox, 1, errno);
        Atomics.store(mailbox, 0, 1);
        Atomics.notify(mailbox, 0);
      }
    }
  };
  worker.postMessage({thread, argument, ...runtime, reply});
  return thread;
}
export {spawn as 'thread-spawn'};
