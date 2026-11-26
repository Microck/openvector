import init, {start_openvector, open_images} from './generated/openvector.js';
import {importImages, wasi} from './browser-host.js';

import {setThreadRuntime} from './threads.js';

try {
  if (!globalThis.crossOriginIsolated) throw new Error('The editor needs HTTPS and cross-origin isolation to start its tracing worker.');
  const module = await WebAssembly.compileStreaming(fetch(new URL(OPENVECTOR_ENGINE_PATH, import.meta.url)));
  const exports = await init({module_or_path: module});
  setThreadRuntime(module, exports.memory);
  wasi.initialize({exports});
  start_openvector();
  window.addEventListener('dragover', event => event.preventDefault());
  window.addEventListener('drop', async event => {
    event.preventDefault();
    if (event.dataTransfer.files.length) open_images(await importImages(event.dataTransfer.files));
  });
} catch (error) {
  const message = document.createElement('p');
  message.setAttribute('role','alert');
  message.textContent = 'OpenVector could not start. ' + error.message;
  document.body.append(message);
  throw error;
}
