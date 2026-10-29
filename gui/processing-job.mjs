const idle = () => ({running:false,status:'idle',current:null,overwrite:null,summary:null,error:null});

export const activeProcessingSteps = {load:'Opening image',classification:'Analyzing image',palette:'Palette analysis',segmentation:'Creating regions',
  smoothing:'Smoothing paths',fitting:'Fitting curves',export:'Saving vector'};
export const completedProcessingSteps = {load:'Opened image',classification:'Analyzed image',palette:'Palette analysis',segmentation:'Created regions',
  smoothing:'Smoothed paths',fitting:'Fitted curves',export:'Saved vector'};

/** Shared processing lifecycle. Each workspace retains its own last result;
 * host admission permits only one active user of the cancellation cell. */
export class ProcessingJob {
  #state = idle();
  #cancellation;
  #publish;
  #abort;

  constructor({cancellation,publish}) {
    this.#cancellation = cancellation;this.#publish = publish;
  }

  get running() {return this.#state.running;}
  get signal() {return this.#abort.signal;}
  get hasSummary() {return this.#state.summary !== null;}
  snapshot() {return structuredClone(this.#state);}

  begin() {
    if (this.running) throw new Error('Wait for processing to finish.');
    Atomics.store(this.#cancellation,0,0);
    this.#abort = new AbortController();
    this.#state = {...idle(),running:true,status:'running'};
    this.#publish();
  }

  observeCancellation() {
    if (Atomics.load(this.#cancellation,0)) this.#abort.abort();
  }

  cancel() {
    if (!this.running) return;
    Atomics.store(this.#cancellation,0,1);
    this.#abort.abort();this.#state.status = 'cancelling';
  }

  progress(event,complete = false) {
    // Cancellation must be observed even when the displayed percent repeats.
    this.observeCancellation();
    const previous = this.#state.current;
    if (!complete && previous && previous.index === event.index && previous.stage === event.stage &&
        previous.progress === event.percent && !this.signal.aborted) return;
    this.#state.current = {index:event.index,filename:event.filename,stage:event.stage,
      progress:event.percent,result:event.status,complete};
    if (this.signal.aborted) this.#state.status = 'cancelling';
    this.#publish();
  }

  waitForDecision(outputs) {
    this.#state.status = 'overwrite';this.#state.overwrite = outputs;this.#publish();
  }

  resume() {
    this.#state.overwrite = null;this.#state.status = 'running';this.#publish();
  }

  finish(status,{summary = null,error = null} = {}) {
    this.#state.running = false;this.#state.status = status;
    this.#state.summary = summary;this.#state.error = error;this.#state.overwrite = null;
    this.#publish();
  }
}
