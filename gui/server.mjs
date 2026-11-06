import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { homedir } from 'node:os';
import { Worker } from 'node:worker_threads';
import { pickPaths, revealFolder,readEmfDevice } from './native-paths.mjs';

const root = fileURLToPath(new URL('.', import.meta.url));
const operations = new Set(['open', 'leavePalette', 'reset', 'resetApplicationModel', 'outputOptions', 'autoUpdatePreferences', 'reviewPreferences', 'quickSave', 'bitmapSave', 'bitmapSize', 'quickSaveFile', 'quickSaveDirectory', 'select', 'palette', 'palettes', 'process', 'vectors', 'automatic', 'review', 'edit', 'background', 'profiles', 'sources']);

/** Loopback reference-test host. CPU work belongs to a worker, keeping API reads
 * responsive. Requests are serialized against a single owned document. */
export async function startGui({ port = 3741, workingDirectory = process.cwd(),
  profileFilename = resolve(homedir(),'.openvector','profiles.json'),
  sourceFilename = resolve(dirname(profileFilename),'sources.json'),
  preferencesFilename = resolve(dirname(profileFilename),'processing-preferences.json') } = {}) {
  const cancellation = new Int32Array(new SharedArrayBuffer(4));
  const picker = new AbortController();
  const worker = new Worker(new URL('./engine-worker.mjs', import.meta.url),
    {workerData:{applicationDirectory:root,workingDirectory,cancellation,profileFilename,sourceFilename,preferencesFilename}});
  let nextId = 0, busy = false, failure = null, batchState, batchRevision = 0, cancelRequested = false, processingState;
  const pending = new Map();
  const thumbnails = new Map();
  worker.on('message', ({ id, value, error, code, state, sourceResult, applicationResult, kind, assets }) => {
    if (kind === 'processing') {processingState = value;return;}
    if (kind === 'batch') {
      for (const image of assets) thumbnails.set(image.id,Buffer.from(JSON.stringify(image)));
      const members = new Set(value.thumbnails.map(image => image.id));
      for (const id of thumbnails.keys()) if (!members.has(id)) thumbnails.delete(id);
      if (cancelRequested && value.job.running) value.job.status = 'cancelling';
      if (!value.job.running) cancelRequested = false;
      batchState = {...value,revision:++batchRevision};return;
    }
    const promise = pending.get(id);
    if (!promise) return;
    pending.delete(id);
    if (error) { const workerError = new Error(error); workerError.code=code;workerError.state = state;workerError.sourceResult=sourceResult;workerError.applicationResult=applicationResult;promise.reject(workerError); }
    else promise.resolve(value);
  });
  const failWorker = error => {
    failure ??= error;
    for (const promise of pending.values()) promise.reject(error);
    pending.clear();
    if (batchState) batchState = {...batchState,job:{...batchState.job,running:false,status:'error',error:error.message,overwrite:null}};
    if (processingState) processingState = {...processingState,running:false,status:'error',error:error.message};
  };
  worker.on('error', failWorker);
  worker.on('exit', code => failWorker(new Error(`Processing worker stopped (${code}). Restart OpenVector.`)));
  const call = (action, input) => {
    if (failure) return Promise.reject(failure);
    const id = nextId++;
    return new Promise((resolveCall, reject) => {
      pending.set(id, { resolve: resolveCall, reject });
      worker.postMessage({ id, action, input });
    });
  };

  let origin;
  const json = (response, status, value) => {
    response.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
    response.end(JSON.stringify(value));
  };
  const processingSnapshot = () => processingState.running && Atomics.load(cancellation,0)
    ? {...processingState,status:'cancelling'} : processingState;
  const readInput = async request => {
    const chunks = [];let size = 0;
    for await (const chunk of request) {
      size += chunk.length;
      if (size > 96*1024*1024) throw new Error('Request exceeds the 96 MB upload limit.');
      chunks.push(chunk);
    }
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  };
  const server = createServer(async (request, response) => {
    // Binding to loopback plus exact Host/Origin checks prevents another web
    // page from driving this local application through DNS rebinding or POSTs.
    if (request.headers.host !== new URL(origin).host ||
        (request.headers.origin && request.headers.origin !== origin)) {
      json(response, 403, { error: 'This GUI accepts requests from its own local window only.' }); return;
    }
    const url = new URL(request.url, origin);
    if (url.pathname === '/api/processing') {
      if (request.method === 'GET') {json(response,200,processingSnapshot());return;}
      if (request.method !== 'POST') {response.writeHead(405);response.end();return;}
      try {
        const input = await readInput(request);
        if (!input || Object.keys(input).length !== 1 || input.command !== 'cancel') throw new Error('Choose Cancel for the active processing operation.');
        if (failure) throw failure;
        if (processingState.running) {
          Atomics.store(cancellation,0,1);
          worker.postMessage({id:null,action:'cancelProcessing'});
        }
        json(response,200,processingSnapshot());
      } catch (error) {json(response,400,{error:error.message});}
      return;
    }
    if (url.pathname === '/api/batch-thumbnail' && request.method === 'GET') {
      const image = thumbnails.get(url.searchParams.get('id'));
      if (!image) {json(response,404,{error:'Thumbnail not found.'});return;}
      response.writeHead(200,{'Content-Type':'application/json','Cache-Control':'private, max-age=31536000, immutable'});
      response.end(image);return;
    }
    if (url.pathname === '/api/batch') {
      if (request.method === 'GET') {json(response,200,batchState);return;}
      if (request.method !== 'POST') {json(response,404,{error:'Unknown GUI operation.'});return;}
      let held = false;
      try {
        const input = await readInput(request);
        if (input === null || typeof input !== 'object' || Array.isArray(input)) throw new Error('Choose a batch command.');
        if (failure) throw failure;
        if (input.command === 'cancel' && batchState.job.running) {
          Atomics.store(cancellation,0,1);
          cancelRequested = true;
          batchState = {...batchState,revision:++batchRevision,job:{...batchState.job,status:'cancelling',overwrite:null}};
          worker.postMessage({id:null,action:'batch',input});
          json(response,200,batchState);return;
        }
        if (busy || batchState.job.running && input.command !== 'decide') {
          json(response,409,{error:'An operation is running. Wait for it to finish.'});return;
        }
        busy = true;held = true;
        if (input.command === 'pick-files') {
          const filenames = await pickPaths('files',batchState.queue.exportDirectory,picker.signal);
          if (filenames.length) await call('batch',{command:'add',filenames});
        } else if (input.command === 'pick-folder') {
          const directories = await pickPaths('folder',batchState.queue.exportDirectory,picker.signal);
          if (directories.length) await call('batch',{command:'configure',exportDirectory:directories[0]});
        } else if (input.command === 'reveal-folder') {
          if (!batchState.job.summary) throw new Error('Complete a batch before opening its export folder.');
          await revealFolder(batchState.job.summary.queue.exportDirectory,workingDirectory);
        }
        else await call('batch',input);
        json(response,200,batchState);
      } catch (error) {json(response,400,{error:error.message});}
      finally {if (held) busy = false;}
      return;
    }
    if (url.pathname.startsWith('/api/')) {
      const action = url.pathname.slice(5);
      const listRead = request.method === 'GET' && ['profiles','sources','autoUpdatePreferences','reviewPreferences','quickSave','bitmapSave','bitmapSize'].includes(action);
      const valid = (request.method === 'GET' && ['state', 'export', 'profiles', 'sources','autoUpdatePreferences','reviewPreferences','quickSave','bitmapSave','bitmapSize'].includes(action)) ||
        (request.method === 'POST' && operations.has(action));
      if (!valid) { json(response, 404, { error: 'Unknown GUI operation.' }); return; }
      if (batchState.job.running && !listRead || batchState.active && !listRead && !['state','profiles','resetApplicationModel','outputOptions','autoUpdatePreferences','reviewPreferences','quickSave'].includes(action)) {
        json(response,409,{error:'Return to single-image mode after batch processing finishes.'});return;
      }
      const mutation = action !== 'state' && !listRead;
      if (busy && mutation) { json(response, 409, { error: 'An operation is running. Wait for it to finish.' }); return; }
      if (mutation) busy = true;
      try {
        let input;
        if (listRead && ['profiles','sources'].includes(action)) input = {command:'list'};
        if (action === 'export') {
          input={format:url.searchParams.get('format')};
          for(const name of ['layout','curveMode','thinStroke','compact'])if(url.searchParams.has(name))
            input[name]=['thinStroke','compact'].includes(name)?JSON.parse(url.searchParams.get(name)):url.searchParams.get(name);
          for(const name of ['scale','flatten'])if(url.searchParams.has(name))input[name]=JSON.parse(url.searchParams.get(name));
          if(input.format==='emf')input.referenceDevice=await readEmfDevice(picker.signal);
        }
        if (request.method === 'POST') {
          input = await readInput(request);
          if (action === 'open') {
            if (typeof input?.bytes !== 'string') throw new Error('Upload image bytes as base64.');
            input.bytes = Buffer.from(input.bytes, 'base64');
            if (input.bytes.length > 64 * 1024 * 1024) throw new Error('Choose an image smaller than 64 MB.');
          }
          if(action==='quickSaveFile') {
            if(Object.hasOwn(input??{},'referenceDevice'))throw new Error('Quick Save uses the host display metrics.');
            if(input?.command==='save' && input.kind==='vector') {
              const choices=await call('quickSave');
              if(choices[input.mode]?.format==='emf')input.referenceDevice=await readEmfDevice(picker.signal);
            }
          }
        }
        let value;
        if(action==='quickSaveDirectory') {
          const directory=await call(action,input),directories=await pickPaths('folder',directory,picker.signal);
          const choices=input.kind==='bitmap'?'bitmapSave':'quickSave';
          value=directories.length ? await call(choices,{mode:input.mode,directory:directories[0]}) : await call(choices);
        } else if (action==='sources' && ['pick','pick-directory','reveal-directory'].includes(input?.command)) {
          const history=await call('sources',{command:'list'});
          const directories=history.directoryHistory.directories;
          if(input.command!=='pick' && (!Number.isInteger(input.index) || input.index<0 || input.index>=directories.length))
            throw new Error('Recent directory not found.');
          const directory=input.command==='pick'?history.directory:directories[input.index];
          if(input.command==='reveal-directory') {await revealFolder(directory,workingDirectory);value={history};}
          else {
            const filenames=await pickPaths('image',directory,picker.signal);
            value=filenames.length ? await call('sources',{...input,command:'open',filename:filenames[0]}) : {history};
          }
        } else value = await call(action, action === 'state' ? true : input);
        // The host owns revisions and thumbnail projection. Profile selection
        // publishes batch state before its reply on the same worker channel.
        if ((action === 'profiles' || action==='resetApplicationModel') && value.batch) value.batch = batchState;
        if (action === 'export') {
          response.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Cache-Control': 'no-store',
            'Content-Disposition': `attachment; filename*=UTF-8''${encodeURIComponent(value.filename)}` });
          response.end(Buffer.from(value.bytes));
        } else json(response, 200, value);
      } catch (error) {
        if (error.applicationResult) error.applicationResult.batch=batchState;
        json(response, 400, { error: error.message, code:error.code, state: error.state,sourceResult:error.sourceResult,applicationResult:error.applicationResult });
      }
      finally { if (mutation) busy = false; }
      return;
    }
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      response.writeHead(405); response.end(); return;
    }
    response.writeHead(404); response.end('Not found.');
  });
  // The worker publishes initial queue state before acknowledging enter/leave.
  // Readiness is explicit so HTTP requests never see a partial initialization.
  await call('batch',{command:'leave'}).catch(async error => {await worker.terminate();throw error;});
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', resolveListen);
  }).catch(async error => { await worker.terminate(); throw error; });
  origin = `http://127.0.0.1:${server.address().port}`;
  return { url: origin, close: async () => {
    picker.abort();
    server.closeAllConnections();
    await new Promise(resolveClose => server.close(resolveClose));
    try {await call('saveProcessingPreferences',null);}
    finally {await worker.terminate();}
  } };
}

