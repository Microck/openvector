/** Compose the native model reset across its existing owners. Domain state
 * changes synchronously; the two independent stores persist afterward. */
export class ApplicationController {
  #engine;
  #batch;
  #profiles;
  #sources;

  constructor({engine,batch,profiles,sources}) {
    this.#engine=engine;this.#batch=batch;this.#profiles=profiles;this.#sources=sources;
  }

  snapshot() {
    return {document:this.#engine.state(true),selection:this.#engine.selectionState(),
      profiles:{names:this.#profiles.names()},sources:this.#sources.snapshot(),batch:this.#batch.snapshot()};
  }

  async resetModel() {
    if (this.#batch.running) throw new Error('Wait for batch processing to finish.');
    this.#profiles.resetList();this.#sources.resetHistory();
    this.#batch.resetSource();this.#engine.resetProcessing();
    // Both files must get a write attempt. A profile-storage error must not
    // leave the source store silently retaining the pre-reset history.
    const writes=await Promise.allSettled([this.#profiles.persist(),this.#sources.persist()]);
    const failures=writes.flatMap((write,index)=>write.status==='rejected'
      ? [`${index===0?'saved profiles':'source history'}: ${write.reason.message}`] : []);
    const applicationResult=this.snapshot();
    if (failures.length) {
      const error=new Error('Could not save reset settings. '+failures.join('; '));
      error.applicationResult=applicationResult;throw error;
    }
    return applicationResult;
  }
}
