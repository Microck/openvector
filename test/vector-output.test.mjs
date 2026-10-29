import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync, mkdtempSync, rmSync, chmodSync, readdirSync, statSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fork, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { gunzipSync } from 'node:zlib';
import reference from './fixtures/output-shapes-reference.json' with { type:'json' };
import progressReference from './fixtures/export-progress-reference.json' with { type:'json' };
import { assembleOutputShapes, isOutputRegionVisible, exportProgressPercent } from '../src/vector-output.mjs';
import { CoreSession } from '../src/palette.mjs';
import { defaultDocumentSettings } from '../src/document-controls.mjs';
import { CancelTaskException } from '../src/task-cancellation.mjs';
import { serializeSvg, exportSvg } from '../src/svg.mjs';
import { serializePdf, exportPdf } from '../src/pdf.mjs';
import { serializeEps, exportEps } from '../src/eps.mjs';
import { serializeAi, exportAi } from '../src/ai.mjs';
import { serializeDxf, exportDxf } from '../src/dxf.mjs';

test('text serializers retain original bytes at document cancellation and complete through the same collector', async () => {
  const rows=gunzipSync(readFileSync(new URL('./fixtures/export-interruption-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(line=>JSON.parse(line));
  const sources=rows.shift().inputs,documents=new Map();
  const serializers={svg:serializeSvg,pdf:serializePdf,eps:serializeEps,ai:serializeAi,dxf:serializeDxf};
  const exporters={svg:exportSvg,pdf:exportPdf,eps:exportEps,ai:exportAi,dxf:exportDxf};
  const settings={...defaultDocumentSettings(),category:2,quality:1,sensitivity:1,advanced:true,antiAlias:false,
    complexity:6,minimumPixels:0,artifactRejection:2,clusterColors:false,smoothness:6,detectCorners:true,
    curveComplexity:6,smoothingAntiAlias:false,paletteMode:2,backgroundMode:1,background:Float32Array.of(1,1,1,1)};
  let completed=0,cancelled=0;
  for(const row of rows) {
    const {format,layout,style,thinStroke,curveMode=0,cancelAt}=row.request;
    if(format==='emf')continue;
    if(!documents.has(row.source)) {
      const source=sources[row.source];
      assert.deepEqual(source.controls,[2,1,1,1,0,6,0,2,0,6,1,6,0,1,1,1,1,1]);
      const document=new CoreSession().createDocument({...source,bgra:Uint8Array.from(Buffer.from(source.bgra,'base64')),
        hasAlpha:false,shadowControl:0});
      for(const stage of ['segmentation','smoothing','fitting'])assert.equal(document.runTask(stage,settings).state.status,0);
      const state=document.snapshot();
      documents.set(row.source,{document,state,before:structuredClone(state.fitting.geometry)});
    }
    const {document,state}=documents.get(row.source),source=sources[row.source];
    const options={geometry:state.fitting.geometry,width:source.width,height:source.height,palette:state.segmentation.palette,
      layout:['stacked','cutouts','grouped-cutouts'][layout],thinStroke:Boolean(thinStroke),compact:style===0,
      curveMode:['splines','sparse-lines','dense-lines'][curveMode],
      ...(format==='eps'?{creationDate:row.expected.validatedCreationDate}:{})};
    let emitted='',checkpoints=0;
    const operation=document.runExport(onProgress=>{
      serializers[format]({...options,onProgress:percent=>{
        if(++checkpoints===cancelAt)document.requestCancel();
        onProgress(percent);
      }},chunk=>{emitted+=chunk;});
      return 0;
    });
    if(cancelAt) {
      await assert.rejects(operation,CancelTaskException,row.id);
      assert.equal(checkpoints,cancelAt,row.id);
      assert.equal(document.snapshot().cancelled,true,row.id);
      cancelled++;
    } else {
      assert.equal((await operation).status,0,row.id);
      assert.deepEqual(Buffer.from(exporters[format](options)),Buffer.from(row.expected.afterProcessExit,'base64'),row.id);
      completed++;
    }
    assert.deepEqual(Buffer.from(emitted.replaceAll('\n','\r\n'),'latin1'),Buffer.from(row.expected.afterProcessExit,'base64'),row.id);
  }
  assert.deepEqual({completed,cancelled},{completed:282,cancelled:188});
  for(const {state,before} of documents.values())assert.deepEqual(state.fitting.geometry,before);
  const {state}=documents.values().next().value,source=sources['grid-64'];
  const options={geometry:state.fitting.geometry,width:source.width,height:source.height,palette:state.segmentation.palette};
  for(const [format,serialize] of Object.entries(serializers)) {
    const failure=new Error('Output consumer failed.');
    let progress=0;
    assert.throws(()=>serialize({...options,onProgress:()=>{progress++;}},()=>{throw failure;}),error=>error===failure,format);
    assert.equal(progress,0,format);
    assert.throws(()=>serialize(options,null),TypeError,format);
  }
});

const readExportFixture = name => gunzipSync(readFileSync(new URL(`./fixtures/${name}.ndjson.gz`,import.meta.url)))
  .toString().trim().split('\n').map(line=>JSON.parse(line));

function windowsHandleCount(pid) {
  return Number(execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',
    `(Get-Process -Id ${pid}).HandleCount`],{encoding:'utf8'}).trim());
}

async function observeFileProcess(t,{sources,requests,errors = false,measureHandles = false,probeAccess = false}) {
  const directory = mkdtempSync(join(tmpdir(),'openvector-vector-files-'));
  const child = fork(new URL('./helpers/vector-file-child.mjs',import.meta.url),[],{stdio:['ignore','pipe','pipe','ipc']});
  let stderr = '';
  child.stderr.on('data',chunk=>{stderr+=chunk;});
  const exited = once(child,'exit');
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) {child.kill();await exited;}
    // Windows read-only fixtures must be made writable before owned cleanup.
    for (const name of readdirSync(directory)) {
      const path = join(directory,name);
      if (statSync(path).isFile()) chmodSync(path,0o666);
    }
    rmSync(directory,{recursive:true,force:true});
  });
  const receive = async expected => {
    const [message] = await once(child,'message');
    assert.equal(message.kind,expected,message.error ?? stderr);
    return message;
  };
  let receiving = receive('ready');
  child.send({kind:'prepare',directory,sources,requests,errors,probeAccess});
  await receiving;
  const before = measureHandles && process.platform === 'win32' ? windowsHandleCount(child.pid) : null;
  receiving = receive('observed');child.send({kind:'run'});
  const {observations} = await receiving;
  const handleGrowth = before === null ? null : windowsHandleCount(child.pid)-before;
  child.send({kind:'exit'});
  const [code,signal] = await exited;
  assert.equal(code,0,stderr);assert.equal(signal,null,stderr);
  return {directory,observations,handleGrowth};
}

// EMF headers record the capturing machine's reference device. That is
// environment metadata, recovered from a completed original EMF in the same
// capture; geometry still comes from the independent pipeline.
function emfDevice(encoded) {
  const header = Buffer.from(encoded,'base64');
  return {pixels:[header.readInt32LE(72),header.readInt32LE(76)],millimeters:[header.readInt32LE(80),header.readInt32LE(84)],
    micrometers:[header.readInt32LE(100),header.readInt32LE(104)]};
}

function assertFileBytes(actual, encoded, label) {
  const expected = Buffer.from(encoded,'base64');
  assert.ok(actual.equals(expected),`${label}: file bytes differ (${actual.length} actual, ${expected.length} expected)`);
}

test('vector file ownership matches native cancellation, same-target retry and open-failure captures', {timeout:300000}, async t => {
  const [metadata,...rows] = readExportFixture('export-interruption-reference');
  const referenceDevice = emfDevice(rows.find(row=>row.request.format==='emf' && !row.request.cancelAt).expected.afterProcessExit);
  const requests = rows.map((row,index)=>({...row.request,id:row.id,source:row.source,referenceDevice,
    filename:`${index}.${row.request.format}`,creationDate:row.expected.validatedCreationDate}));
  const files = await observeFileProcess(t,{sources:metadata.inputs,requests});
  let complete = 0,cancelled = 0;
  for (const [index,row] of rows.entries()) {
    const observed = files.observations[index],request = requests[index];
    assert.equal(observed.caught,Boolean(request.cancelAt),row.id);
    assert.equal(observed.methodResult,request.cancelAt ? null : 0,row.id);
    assert.equal(observed.ioError,null,row.id);
    if (request.cancelAt) {assert.equal(observed.checkpoints,request.cancelAt,row.id);cancelled++;}
    else {assertFileBytes(Buffer.from(observed.file.bytes,'base64'),row.expected.afterProcessExit,`${row.id} at return`);complete++;}
    assertFileBytes(readFileSync(join(files.directory,request.filename)),row.expected.afterProcessExit,`${row.id} after exit`);
  }
  assert.deepEqual({complete,cancelled},{complete:390,cancelled:270});

  // Retained EMF handles share nothing, so on Windows the child cannot read
  // them at unwind, matching the original. Other platforms have no share
  // modes, so an EMF the original could not read is readable there.
  const assertAtUnwind = (observed, expected, format, label) => {
    if (expected.readable) {
      assert.equal(observed.readable,true,label);
      assertFileBytes(Buffer.from(observed.bytes,'base64'),expected.bytes,`${label} at unwind`);
    } else if (process.platform === 'win32' || format !== 'emf') assert.equal(observed.readable,false,label);
  };
  for (const resourceFixture of ['export-stream-buffer-reference','export-large-path-reference']) {
    const [resourceMetadata,...textResources] = readExportFixture(resourceFixture);
    const resourceRequests = textResources.map(row=>({...row.request,id:row.id,source:'grid',filename:`${row.id}.${row.request.format}`,referenceDevice,
      creationDate:/^%%CreationDate: (.+)\r$/m.exec(Buffer.from(row.expected.afterProcessExit,'base64').toString('latin1'))?.[1]}));
    const resourceFiles = await observeFileProcess(t,{sources:{grid:resourceMetadata.source},requests:resourceRequests,measureHandles:true});
    for (const [index,row] of textResources.entries()) {
      const expected = row.expected.documentMethod,observed = resourceFiles.observations[index];
      // EPS receives its already interval-validated captured clock as metadata.
      // Neither the current output nor its reference bytes are normalized.
      assertAtUnwind(observed.file,expected.fileAtUnwind,row.request.format,row.id);
      assertFileBytes(readFileSync(join(resourceFiles.directory,resourceRequests[index].filename)),row.expected.afterProcessExit,`${row.id} after exit`);
      assert.equal(observed.progress,expected.state.progress,row.id);
      assert.equal(observed.cancelled,Boolean(expected.state.cancelled),row.id);
    }
    if (process.platform === 'win32') assert.equal(resourceFiles.handleGrowth,
      textResources.reduce((sum,row)=>sum+row.expected.handleGrowth,0),`${resourceFixture}: actual retained file handles`);
  }

  const [traceMetadata,...traces] = readExportFixture('export-file-state-reference');
  for (const trace of traces) {
    const steps = trace.steps;
    const traceRequests = steps.map(({request,expected})=>{
      const header = Buffer.from(expected.fileAtUnwind.bytes ?? '','base64').toString('latin1');
      const creationDate = /^%%CreationDate: (.+)\r$/m.exec(header)?.[1];
      return {...request,source:'grid',creationDate,referenceDevice};
    });
    const saved = await observeFileProcess(t,{sources:{grid:traceMetadata.source},requests:traceRequests,
      errors:trace.id==='file-errors',measureHandles:true});
    for (const [index,step] of steps.entries()) {
      const actual = saved.observations[index],expected = step.expected;
      assert.equal(actual.methodResult,expected.methodResult,step.request.id);
      assert.equal(actual.caught,Boolean(expected.caught),step.request.id);
      assert.equal(actual.progress,expected.state.progress,step.request.id);
      assertAtUnwind(actual.file,expected.fileAtUnwind,step.request.format,step.request.id);
    }
    for (const [name,expected] of Object.entries(trace.finalFiles)) {
      // An EMF retry fails only because the cancelled save still holds an
      // unshared Windows handle. Without share modes the retry rewrites it.
      if (process.platform !== 'win32' && trace.id === 'same-target' && name.endsWith('.emf')) continue;
      const filename = join(saved.directory,name);
      if (expected.kind==='absent') assert.equal(existsSync(filename),false,`${trace.id}/${name} remains absent`);
      else if (expected.kind==='directory') assert.equal(statSync(filename).isDirectory(),true,name);
      else {
        assertFileBytes(readFileSync(filename),expected.payload,`${trace.id}/${name} after exit`);
        assert.equal((statSync(filename).mode & 0o200)===0,expected.readOnly,name);
      }
    }
    if (process.platform==='win32') assert.equal(saved.handleGrowth,steps.filter(step=>step.request.cancelAt).length,trace.id);
  }
});

// Native probes opened each output right after its save returned or unwound,
// with OPEN_EXISTING and full sharing. The child repeats those probes from a
// separate PowerShell process, so the file owner under test never measures
// itself. Share modes only exist on Windows.
test('Windows file sharing matches native access probes after each save', {timeout:300000,
  skip:process.platform !== 'win32' && 'share modes are Windows-only'}, async t => {
  const [metadata,...groups] = readExportFixture('export-file-access-reference');
  const referenceDevice = emfDevice(readExportFixture('export-interruption-reference').slice(1)
    .find(row=>row.request.format==='emf' && !row.request.cancelAt).expected.afterProcessExit);
  const project = row => ({desiredAccess:row.desiredAccess,opened:row.opened,error:row.error,size:row.size});
  let probes = 0;
  for (const group of groups) {
    // These captures keep no bytes; they share the interruption capture's host.
    const requests = group.requests.map(request=>({...request,source:'grid',referenceDevice}));
    const saved = await observeFileProcess(t,{sources:{grid:metadata.source},requests,
      errors:group.id==='file-errors',probeAccess:true});
    for (const [index,request] of requests.entries()) {
      assert.deepEqual(saved.observations[index].access.map(project),group.access[index].opens,`${group.id}/${request.id}`);
      probes += group.access[index].opens.length;
    }
  }
  assert.equal(probes,348);
});

test('all 51 export progress phases match the original document integer observer', () => {
  assert.deepEqual(progressReference.phases.map(frame => frame.phase), Array.from({length:51}, (_,phase) => phase));
  for (const frame of progressReference.phases) {
    assert.equal(frame.cancelled, false);
    assert.equal(exportProgressPercent(frame.phase), frame.percent, `phase ${frame.phase}`);
  }
});

test('output child/hole paths and color groups match all 2872 original states', () => {
  assert.equal(reference.cases.length, 2872);
  let base = 0, visibility = 0;
  for (const [index, fixture] of reference.cases.entries()) {
    const { visibleQueries, ...expected } = fixture.expected;
    assert.deepEqual(assembleOutputShapes(fixture.geometry), expected, `original output assembly ${index}`);
    assert.equal(visibleQueries.length, fixture.geometry.regions.length + 4);
    if (fixture.visibilityProbe) visibility++; else base++;
  }
  assert.equal(base, 2712); assert.equal(visibility, 160);
});

test('output visibility matches native hidden words across range and word boundaries', () => {
  const limits = new Set(), patterns = new Set();
  for (const fixture of reference.cases) {
    const actual = fixture.expected.visibleQueries.map((_, region) =>
      isOutputRegionVisible(fixture.hiddenSize, fixture.hiddenWords, region));
    assert.deepEqual(actual, fixture.expected.visibleQueries);
    limits.add(fixture.hiddenSize);
    if (fixture.visibilityProbe) patterns.add(fixture.visibilityProbe);
  }
  for (const boundary of [0, 1, 31, 32, 33, 63, 64, 65, 1023, 1024, 1025]) assert.ok(limits.has(boundary));
  assert.deepEqual([...patterns].sort(), ['all', 'boundaries', 'even', 'none', 'odd']);
});

test('assembled output arrays own their paths and preserve the fitted graph', () => {
  const fixture = reference.cases.find(frame => frame.expected.holes.some(paths => paths.length > 0));
  const geometry = structuredClone(fixture.geometry), before = structuredClone(geometry);
  const first = assembleOutputShapes(geometry), second = assembleOutputShapes(geometry);
  const region = first.holes.findIndex(paths => paths.length > 0);
  first.holes[region][0][0][0] = -1;
  first.children[region][0] = -1;
  first.colorOrder[0][0] = -1;
  const { visibleQueries, ...expected } = fixture.expected;
  assert.deepEqual(second, expected);
  assert.deepEqual(geometry, before);
});
