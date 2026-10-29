import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { basename, dirname, resolve, sep } from 'node:path';
import { CoreSession } from './palette.mjs';
import { ImageDocument, CancelTaskException } from './image-document.mjs';
import { BatchQueue } from './batch-queue.mjs';
import { readBatchControls, advancedBatchControls } from './document-controls.mjs';
import { decodeImage, resolveImageFilename } from './image-input.mjs';
import { writeVectorFile } from './vector-file.mjs';
import {readExportOptions} from './export-options.mjs';

export { BatchQueue };

const formats = new Set(['svg', 'eps', 'pdf', 'ai', 'dxf']);
const categories = ['Photograph', 'Artwork with blending', 'Artwork without blending', 'Auto-detect'];
const qualities = ['High', 'Medium', 'Low', 'Auto-detect'];
const palettes = ['Two colors', 'Fixed Palette', 'Flexible Palette', 'Auto-detect'];
const backgrounds = { 0: 'Preserve transparency', 1: 'Flatten to white', 3: 'Auto-detect' };
const stageLabels = { classification: 'classifying', palette: 'finding palette', segmentation: 'segmenting', smoothing: 'smoothing', fitting: 'fitting' };
const batchSettingNames = new Set(['advanced', 'category', 'quality', 'paletteMode', 'colorCount', 'backgroundMode', 'palette', ...advancedBatchControls]);
const nativePath = path => path.replaceAll('/', sep);
const clock = () => new Date().toTimeString().slice(0, 8);
const duration = milliseconds => `${Math.floor(milliseconds / 60000)}:${String(Math.floor(milliseconds / 1000) % 60).padStart(2, '0')}.${String(Math.floor(milliseconds) % 1000).padStart(3, '0')}`;

function settingsLog(settings, detected = false) {
  if (settings.advanced) {
    if (!detected) return `Saved Advanced Settings\nTransparency Mode: ${backgrounds[settings.backgroundMode]}`;
    return settings.paletteMode === 1 ? `Number of Colors: ${settings.palette.length / 4}` : '';
  }
  const lines = [`Image Category: ${categories[settings.category]}`, `Image Quality: ${qualities[settings.quality]}`,
    `Palette Mode: ${palettes[settings.paletteMode]}`];
  if (!detected || settings.paletteMode !== 2) {
    const count = detected ? settings.palette.length / 4 : settings.colorCount;
    lines.push(`Number of Colors: ${count || 'Auto-detect'}`);
  }
  lines.push(`Transparency Mode: ${backgrounds[settings.backgroundMode]}`);
  return lines.join('\n');
}

export function readBatchConfiguration(settings, exportOptions) {
  if (settings === null || typeof settings !== 'object' || Array.isArray(settings))
    throw new TypeError('Batch settings must be an object.');
  for (const key of Object.keys(settings)) if (!batchSettingNames.has(key))
    throw new Error('Unknown batch setting: ' + key);
  const controls = readBatchControls(settings);
  const options=readExportOptions(exportOptions);
  return { controls, options };
}

/** Filesystem execution shared by the CLI and local application controller.
 * One retained session owns the random stream across files and subsequent jobs.
 * Load/save failures are per-file outcomes; processing failures stay visible. */
export class BatchRunner {
  #queue;
  #session;
  #document = null;
  #running = false;

  constructor({ queue, session, document }) {
    if (!(queue instanceof BatchQueue) || session !== undefined && !(session instanceof CoreSession))
      throw new TypeError('BatchRunner requires a BatchQueue and CoreSession.');
    if (document !== undefined && !(document instanceof ImageDocument))
      throw new TypeError('An existing batch document must be an ImageDocument.');
    if (document !== undefined && session !== undefined)
      throw new TypeError('Choose an existing document or a session for batch ownership.');
    this.#queue = queue;
    this.#session = document === undefined ? session ?? new CoreSession() : null;
    this.#document = document ?? null;
  }

  /** Controller access to the actual document. The host serializes document
   * operations while a job runs; consumers needing owned data use snapshot(). */
  get document() {
    if (this.#running) throw new Error('Wait for the batch job to finish before adopting its document.');
    return this.#document;
  }

  snapshot() {
    return { running: this.#running, queue: this.#queue.snapshot(), document: this.#document?.snapshot() ?? null };
  }

  async run({ settings = {}, exportOptions = {}, decideOverwrite = () => 'skip', onStage, onProgress, signal } = {}) {
    const { controls, options } = readBatchConfiguration(settings, exportOptions);
    const format = this.#queue.exportFormat;
    if (!formats.has(format)) throw new Error('Batch format must be svg, eps, pdf, ai or dxf.');
    if (typeof decideOverwrite !== 'function' || onStage !== undefined && typeof onStage !== 'function' ||
      onProgress !== undefined && typeof onProgress !== 'function')
      throw new TypeError('Batch decision, stage and progress observers must be functions.');
    if (signal !== undefined && !(signal instanceof AbortSignal)) throw new TypeError('Batch signal must be an AbortSignal.');
    if (this.#running) throw new Error('This batch runner is already processing a queue.');
    this.#running = true;
    const sections = [], files = [];
    const interrupted = new Error('Batch processing was cancelled.');
    const checkCancelled = () => {
      if (signal?.aborted) {
        this.#document?.requestCancel();
        throw interrupted;
      }
    };
    const result = status => ({ status, files, queue: this.#queue.snapshot(), log: sections.join('\n\n'),
      converted: files.filter(file => file.status === 'written').length,
      failed: files.filter(file => file.status === 'failed-load' || file.status === 'failed-save').length,
      skipped: files.filter(file => file.status === 'skipped').length });
    try {
      if (!this.#queue.snapshot().queue.length) return result('empty');
      if (signal?.aborted) return result('cancelled');
      const began = performance.now(), startClock = clock();
      const absolute = filename => resolve(this.#queue.workingDirectory, filename);
      const planned = this.#queue.prepare(filename => existsSync(absolute(filename)));
      const before = this.#queue.snapshot();
      sections.push(`Starting batch processing of ${before.queue.length} images...`, `Start time: ${startClock}`,
        `Export directory: '${nativePath(before.exportDirectory)}'`);
      const existing = new Set(planned.overwrites);
      for (const record of before.queue) sections.push(`Bitmap Filename: '${basename(record.filename)}'\n` +
        `Bitmap Directory: '${nativePath(dirname(absolute(record.filename)))}'\n` +
        `Vector Filename: '${basename(record.outputFilename)}'\nWould Overwrite: ${existing.has(record.outputFilename)}`);
      let decision = 'skip';
      if (planned.overwrites.length) {
        sections.push('Some files would be overwritten.\nAsking the user what to do...');
        decision = await decideOverwrite(planned.overwrites.slice());
      }
      if (signal?.aborted) decision = 'cancel';
      const chosen = this.#queue.decideOverwrite(decision);
      for (const record of this.#queue.snapshot().queue) files.push({ filename: record.filename,
        outputFilename: record.outputFilename, status: record.shouldProcess ? 'pending' : 'skipped' });
      if (decision === 'cancel') return result('cancelled');
      if (planned.overwrites.length) sections[sections.length - 1] += decision === 'overwrite'
        ? '\nThe user has decided to overwrite any existing files.'
        : '\nThe user has decided to skip any files that would overwrite exising files.';
      if (!chosen.shouldStart) return result('no-files');
      sections.push('Using the following settings:', settingsLog(controls), 'Starting to process images...');
      let completingExport = false;
      for (const [index, file] of files.entries()) {
        this.#queue.processingIndex = index;
        if (file.status === 'skipped') continue;
        const lines = [`Converting '${basename(file.filename)}' to '${basename(file.outputFilename)}'...`];
        // Every attempted file contributes exactly one log section, including
        // when cancellation leaves its current task without a completion log.
        try {
          let decoded;
          const metadata = resolveImageFilename(file.filename);
          let loadFailure;
          try {
            this.#document ??= this.#session.createDocument();
            await this.#document.runBatchLoad(controls,async () => {
              // Native requests the next load inside a successful export's
              // completion signal. The still-active scheduler cancels that
              // task after source reset; loading itself does not clear it.
              if (completingExport) this.#document.requestCancel();
              completingExport = false;
              const encoded = await readFile(absolute(file.filename));
              decoded = decodeImage(encoded);
              return { ...decoded, shadowControl: metadata.shadowControl };
            });
          } catch (error) {
            if (error === interrupted) throw error;
            loadFailure = error;
          }
          onStage?.({ index, filename: file.filename, stage: 'load', state: this.#document.snapshot(),
            source: loadFailure ? null : {format:decoded.format,hasAlpha:decoded.hasAlpha} });
          if (signal?.aborted) {
            this.#document.cancelLoad();
            throw interrupted;
          }
          if (loadFailure) {
            file.status = 'failed-load';file.error = loadFailure.message;
            lines.push('Failed To Open File.');continue;
          }
          let previous = performance.now();
          const processed = this.#document.runBatchImage({ ...controls, extension: metadata.extension,
            onProgress:event => {
              onProgress?.({index,filename:file.filename,...event});
              // Let the document throw at its native checkpoint, preserving
              // partial candidates and consumed randomness before unwinding.
              if (signal?.aborted) this.#document.requestCancel();
            } }, (stage, state, selection) => {
            const now = performance.now();
            if (stage === 'segmentation') {
              const detected = settingsLog(state.settings, true);
              if (detected) lines.push(detected);
            }
            onStage?.({ index, filename: file.filename, stage, state,...selection });
            checkCancelled();
            lines.push(`Done ${stageLabels[stage]}: ${duration(now - previous)}`);
            previous = now;
          });
          let saveFailure = null;
          const exported = await this.#document.runExport(async onProgress => {
            // Native preflight determines eligibility; opening replaces aliases
            // or files created afterwards, before the serializer visits regions.
            const saved = writeVectorFile(absolute(file.outputFilename), { ...options,
              geometry: processed.state.fitting.geometry, width: decoded.width, height: decoded.height,
              palette: processed.state.segmentation.palette, onProgress });
            saveFailure = saved.status === 0 ? null : saved.error;
            return saved.status;
          });
          completingExport = !saveFailure;
          if (!saveFailure) file.status = 'written';
          onStage?.({ index, filename: file.filename, stage: 'export', state: exported });
          checkCancelled();
          if (saveFailure) {
            file.status = 'failed-save';file.error = saveFailure.message;
            lines.push('Failed To Save File.');
          }
        } finally { sections.push(lines.join('\n')); }
      }
      sections.push(`End time: ${clock()}\nElapsed time: ${duration(performance.now() - began)}`, 'Done processing images.');
      return result('complete');
    } catch (error) {
      if (error !== interrupted && !(signal?.aborted && error instanceof CancelTaskException)) throw error;
      return result('cancelled');
    } finally {
      this.#running = false;
    }
  }
}
