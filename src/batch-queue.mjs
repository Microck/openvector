import { win32 } from 'node:path';

/** Shared queue state. File decoding, conversion and dialogs belong to the
 * application controller; preparing a queue only observes output existence. */
export class BatchQueue {
  #records = [];
  #workingDirectory;
  #directory;
  #format = 'ai';
  #acceptingEdits = true;
  #processingIndex = 0;

  constructor({ applicationDirectory, workingDirectory = process.cwd() }) {
    if (typeof applicationDirectory !== 'string' || typeof workingDirectory !== 'string')
      throw new TypeError('Batch application and working directories must be strings.');
    this.#directory = applicationDirectory.replaceAll('\\', '/');
    this.#workingDirectory = workingDirectory;
  }

  get exportDirectory() { return this.#directory; }
  get workingDirectory() { return this.#workingDirectory; }
  set exportDirectory(value) {
    if (typeof value !== 'string') throw new TypeError('Batch export directory must be a string.');
    this.#directory = value;
  }
  get exportFormat() { return this.#format; }
  set exportFormat(value) {
    if (typeof value !== 'string') throw new TypeError('Batch export format must be a string.');
    this.#format = value;
  }
  get processingIndex() { return this.#processingIndex; }
  set processingIndex(value) {
    if (!Number.isInteger(value) || value < -2147483648 || value > 2147483647)
      throw new TypeError('Batch processing index must be a signed int32 integer.');
    this.#processingIndex = value;
  }

  snapshot() {
    return { exportDirectory: this.#directory, exportFormat: this.#format,
      processingIndex: this.#processingIndex, acceptingEdits: this.#acceptingEdits,
      queue: this.#records.map((record, buttonIndex) => ({ ...record, buttonIndex })) };
  }

  addFiles(filenames) {
    if (!Array.isArray(filenames) || filenames.some(filename => typeof filename !== 'string'))
      throw new TypeError('Batch filenames must be an array of strings.');
    const known = new Set(this.#records.map(record => record.filename)), duplicates = [];
    for (const filename of filenames) {
      if (known.has(filename)) { duplicates.push(filename); continue; }
      known.add(filename);
      this.#records.push({ filename, selected: false, outputFilename: '', shouldProcess: false });
      // Native QFileInfo::absolutePath applies Windows rules on every input:
      // both separators split, and drive or rooted names ignore the base.
      this.#directory = win32.dirname(win32.resolve(this.#workingDirectory, filename)).replaceAll('\\', '/');
    }
    return duplicates;
  }

  #record(index) {
    if (!Number.isInteger(index) || index < 0 || index >= this.#records.length)
      throw new RangeError('Batch record index is out of range.');
    return this.#records[index];
  }

  select(index, selected) {
    const record = this.#record(index);
    if (typeof selected !== 'boolean') throw new TypeError('Batch selection must be boolean.');
    if (this.#acceptingEdits) record.selected = selected;
  }

  setAcceptingEdits(value) {
    if (typeof value !== 'boolean') throw new TypeError('Batch editability must be boolean.');
    this.#acceptingEdits = value;
    if (!value) for (const record of this.#records) record.selected = false;
  }

  remove(index) {
    if (this.#record(index).selected) this.removeSelected();
    else this.#records.splice(index, 1);
  }

  removeSelected() { this.#records = this.#records.filter(record => !record.selected); }
  clear() { this.#records = []; }

  /** Allocate names from queue membership, independently of existing files.
   * The native QString set is case-sensitive, even on Windows. completeBaseName
   * drops the last suffix from dotfiles too, so .hidden becomes an empty stem. */
  prepare(exists) {
    if (typeof exists !== 'function') throw new TypeError('Batch preparation requires an output-existence reader.');
    const names = new Set(), overwrites = [];
    for (const record of this.#records) {
      const filename = record.filename.replaceAll('\\', '/').split('/').at(-1);
      const dot = filename.lastIndexOf('.');
      const base = dot < 0 ? filename : filename.slice(0, dot);
      let stem = base, suffix = 0;
      while (names.has(stem)) stem = `${base}_${++suffix}`;
      names.add(stem);
      record.outputFilename = `${this.#directory}/${stem}.${this.#format}`;
      record.shouldProcess = !exists(record.outputFilename);
      if (!record.shouldProcess) overwrites.push(record.outputFilename);
    }
    return { overwrites, processable: this.#records.length - overwrites.length };
  }

  /** Apply the one decision made after prepare(). Cancellation preserves the
   * planned eligibility bits; overwrite opts every record into processing. */
  decideOverwrite(decision) {
    if (!['overwrite', 'skip', 'cancel'].includes(decision))
      throw new TypeError('Batch overwrite decision must be overwrite, skip or cancel.');
    let processable = 0;
    for (const record of this.#records) {
      if (decision === 'overwrite') record.shouldProcess = true;
      if (record.shouldProcess) processable++;
    }
    return { shouldStart: decision !== 'cancel' && processable > 0, processable };
  }
}
