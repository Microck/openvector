import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync, chmodSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { CoreSession } from '../../src/palette.mjs';
import { defaultDocumentSettings } from '../../src/document-controls.mjs';
import { CancelTaskException } from '../../src/task-cancellation.mjs';
import { writeVectorFile } from '../../src/vector-file.mjs';

// A real process boundary is required: files observed while the child waits
// for exit differ from files after its retained buffers have drained.
const settings = {...defaultDocumentSettings(),category:2,quality:1,sensitivity:1,advanced:true,antiAlias:false,
  complexity:6,minimumPixels:0,artifactRejection:2,clusterColors:false,smoothness:6,detectCorners:true,
  curveComplexity:6,smoothingAntiAlias:false,paletteMode:2,backgroundMode:1,background:Float32Array.of(1,1,1,1)};
let run;

// Same Win32 probe as the native file-access recorder, run in its own process
// while this child still holds any retained handle.
const accessQuery = fileURLToPath(new URL('./query-export-file-access.ps1',import.meta.url));
const probeAccess = filename => JSON.parse(execFileSync('powershell.exe',
  ['-NoProfile','-NonInteractive','-File',accessQuery,'-Filename',filename],{encoding:'utf8'}));
process.on('message', async message => {
  try {
    if (message.kind === 'prepare') {
      const documents = new Map();
      for (const [name, source] of Object.entries(message.sources)) {
        assert.deepEqual(source.controls,[2,1,1,1,0,6,0,2,0,6,1,6,0,1,1,1,1,1]);
        const document = new CoreSession().createDocument({...source,
          bgra:Uint8Array.from(Buffer.from(source.bgra,'base64')),hasAlpha:false,shadowControl:0});
        for (const stage of ['segmentation','smoothing','fitting']) assert.equal(document.runTask(stage,settings).state.status,0);
        documents.set(name,{document,state:document.snapshot(),source});
      }
      if (message.errors) {
        for (const format of ['svg','pdf','eps','ai','dxf','emf']) {
          mkdirSync(join(message.directory,`blocked.${format}`));
          const filename = join(message.directory,`readonly.${format}`);
          writeFileSync(filename,'owned-readonly-export-marker\n');chmodSync(filename,0o444);
        }
      }
      run = {...message,documents};
      process.send({kind:'ready'});
    } else if (message.kind === 'run') {
      const observations = [];
      for (const request of run.requests) {
        const {document,state,source} = run.documents.get(request.source);
        const filename = join(run.directory,request.filename);
        const options = {geometry:state.fitting.geometry,width:source.width,height:source.height,
          palette:state.segmentation.palette,layout:['stacked','cutouts','grouped-cutouts'][request.layout],
          thinStroke:Boolean(request.thinStroke),compact:request.style===0,
          curveMode:['splines','sparse-lines','dense-lines'][request.curveMode ?? 0],
          ...(request.creationDate ? {creationDate:request.creationDate} : {}),
          ...(request.referenceDevice ? {referenceDevice:request.referenceDevice} : {})};
        let checkpoints = 0, methodResult = null, caught = false, ioError = null;
        try {
          const saved = await document.runExport(onProgress => {
            const outcome = writeVectorFile(filename,{...options,onProgress:percent => {
              if (++checkpoints === request.cancelAt) document.requestCancel();
              onProgress(percent);
            }});
            ioError = outcome.error?.code ?? null;
            return outcome.status;
          });
          methodResult = saved.status;
        } catch (error) {
          if (!(error instanceof CancelTaskException)) throw error;
          caught = true;
        }
        let file;
        try { file = {readable:true,bytes:readFileSync(filename).toString('base64')}; }
        catch (error) { file = {readable:false,error:error.code}; }
        const current = document.snapshot();
        observations.push({id:request.id,methodResult,caught,checkpoints,ioError,file,
          progress:current.progress,cancelled:current.cancelled,
          ...(run.probeAccess ? {access:probeAccess(filename)} : {})});
      }
      process.send({kind:'observed',observations});
    } else if (message.kind === 'exit') process.disconnect();
    else throw new Error('Unknown file-lifecycle test phase.');
  } catch (error) {
    process.send({kind:'failure',error:error.stack});
    process.disconnect();process.exitCode = 1;
  }
});
