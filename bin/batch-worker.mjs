import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';
import { BatchRunner, BatchQueue } from '../src/batch-runner.mjs';

/** The CLI keeps signal handling on its event loop while the shared runner
 * owns CPU work and filesystem handles in one thread. Forced termination has
 * no summary; cooperative cancellation returns the ordinary runner result. */
export function startBatchWorker(queue, { settings = {}, exportOptions = {}, decision = 'skip', signal, onStage, onProgress } = {}) {
  if (!(queue instanceof BatchQueue)) throw new TypeError('A batch worker requires a BatchQueue.');
  if (signal !== undefined && !(signal instanceof AbortSignal)) throw new TypeError('Batch signal must be an AbortSignal.');
  if (onStage !== undefined && typeof onStage !== 'function') throw new TypeError('Batch stage observer must be a function.');
  if (onProgress !== undefined && typeof onProgress !== 'function') throw new TypeError('Batch progress observer must be a function.');
  const synchronization = new Int32Array(new SharedArrayBuffer(8));
  const cancel = () => Atomics.store(synchronization, 0, 1);
  if (signal?.aborted) cancel();
  const worker = new Worker(new URL(import.meta.url), { workerData: {
    queue: queue.snapshot(), workingDirectory: queue.workingDirectory,
    settings, exportOptions, decision, synchronization, observeStages: onStage !== undefined, observeProgress: onProgress !== undefined,
  } });
  signal?.addEventListener('abort', cancel, { once: true });
  let forced = false, failure, summary, received = false;
  const completion = new Promise((resolve, reject) => {
    worker.on('message', message => {
      if (message.kind === 'stage' || message.kind === 'progress') {
        try { (message.kind === 'stage' ? onStage : onProgress)(message.event); }
        catch (error) { failure = { error };void worker.terminate(); }
        finally {
          Atomics.store(synchronization, 1, 1);
          Atomics.notify(synchronization, 1);
        }
      } else if (message.kind === 'error') failure = { error: message.error };
      else { summary = message.summary;received = true; }
    });
    worker.once('error', error => { failure = { error }; });
    // Wait for thread exit so a settled job cannot resume from a stage
    // acknowledgement after its caller has already handled completion.
    worker.once('exit', code => {
      signal?.removeEventListener('abort', cancel);
      if (failure) reject(failure.error);
      else if (forced) resolve(null);
      else if (code !== 0 || !received) reject(new Error(`Batch worker stopped before returning a result (exit ${code}).`));
      else resolve(summary);
    });
  });
  return { completion, terminate: () => { forced = true;void worker.terminate(); } };
}

if (!isMainThread) {
  const { queue: plan, workingDirectory, settings, exportOptions, decision, synchronization, observeStages, observeProgress } = workerData;
  try {
    const queue = new BatchQueue({ applicationDirectory: plan.exportDirectory, workingDirectory });
    queue.addFiles(plan.queue.map(record => record.filename));
    queue.exportDirectory = plan.exportDirectory;queue.exportFormat = plan.exportFormat;
    queue.processingIndex = plan.processingIndex;
    for (const [index, record] of plan.queue.entries()) if (record.selected) queue.select(index, true);
    queue.setAcceptingEdits(plan.acceptingEdits);
    const controller = new AbortController();
    const observeCancellation = () => { if (Atomics.load(synchronization, 0)) controller.abort(); };
    const observe = (kind,event,enabled) => {
      if (enabled) {
        // The same acknowledgement orders completed stages and active-task
        // progress before checking a cancellation requested by the host.
        Atomics.store(synchronization, 1, 0);
        parentPort.postMessage({ kind, event });
        Atomics.wait(synchronization, 1, 0);
      }
      observeCancellation();
    };
    observeCancellation();
    const summary = await new BatchRunner({ queue }).run({ settings, exportOptions,
      decideOverwrite: () => { observeCancellation();return decision; }, signal: controller.signal,
      onStage: event => observe('stage',event,observeStages),
      onProgress: event => observe('progress',event,observeProgress) });
    parentPort.postMessage({ kind: 'result', summary });
  } catch (error) { parentPort.postMessage({ kind: 'error', error }); }
}
