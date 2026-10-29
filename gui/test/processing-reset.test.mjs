import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,writeFile,rm} from 'node:fs/promises';
import {join,dirname,resolve,win32} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {SourceController} from '../source-controller.mjs';
import {quantizeQColorChannels} from '../../src/pixel-input.mjs';
import {profileFromFrame,compareDocumentState} from '../../test/helpers/native-gui.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/processing-reset-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const parameters=JSON.parse(readFileSync(new URL('../../test/fixtures/processing-reset-parameters.json',import.meta.url),'utf8'));
const slots={'setImageCategory(int)':'category','setImageQuality(int)':'quality','setUseAdvancedMode(bool)':'advanced',
  'setPaletteMode(int)':'paletteMode','setAdvColorSensitivity(int)':'sensitivity','setAdvUsePaletteAa(bool)':'antiAlias',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvMinNumPixels(int)':'minimumPixels',
  'setAdvAntiAliasingRejection(int)':'artifactRejection','setAdvClusterColors(bool)':'clusterColors',
  'setAdvContourSmoothness(int)':'smoothness','setAdvDetectSharpCorners(bool)':'detectCorners',
  'setAdvCurveComplexity(int)':'curveComplexity','setAdvUseContourAa(bool)':'smoothingAntiAlias','setFlattenImageMode(int)':'backgroundMode'};

async function context(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-processing-reset-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-processing-reset-')));
    await rm(directory,{recursive:true,force:true});
  });
  const engine=new GuiEngine(),batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))});
  return {directory,engine,batch,profiles:await ProfileController.open({engine,batch,filename:join(directory,'profiles.json')}),
    sources:await SourceController.open({engine,batch,filename:join(directory,'sources.json'),applicationDirectory:directory,workingDirectory:directory})};
}

// Phase zero compares current model behavior. Phase-one native preference
// restoration remains captured evidence for the separate persistence owner.
for(const reference of references)test('native processing reset: '+reference.id,async t=>{
  const {directory,engine,batch,profiles,sources}=await context(t),local=new Map();
  for(const [filename,encoded] of Object.entries(reference.inputs)) {
    const target=join(directory,win32.basename(win32.dirname(filename)),win32.basename(filename));
    await mkdir(dirname(target),{recursive:true});await writeFile(target,Buffer.from(encoded,'base64'));local.set(filename,target);
  }
  for(const [index,[kind,...values]] of reference.phases[0].entries()) {
    if(kind==='open-wide' || kind==='automatic') {
      await sources.command({command:'open',filename:local.get(values[0])});
      if(kind==='automatic')engine.automatic({settings:engine.selectedProfile().settings});
    } else if(kind==='set') {
      const [receiver,signature,value]=values;
      if(signature==='addColor()') {assert.equal(receiver,2);engine.palette({action:'add'});}
      else if(signature==='setAdvNumColors(int)' || signature==='setBatchNumColors(int)') {
        const advanced=signature==='setAdvNumColors(int)';
        if(receiver===2)engine.configureProfile({[advanced?'colorCount':'basicColorCount']:value});
        else {
          // Drive the existing mode-specific controls to assign the native
          // inactive counter, then restore the mode before comparison.
          const prior=batch.selectedProfile().settings.advanced;
          await batch.command({command:'configure',settings:{advanced,colorCount:value}});
          await batch.command({command:'configure',settings:{advanced:prior}});
        }
      } else {
        const key=slots[signature];assert.ok(key,signature);
        const selected=receiver===2?engine.selectedProfile():batch.selectedProfile();
        const settings={[key]:typeof selected.settings[key]==='boolean'?Boolean(value):value};
        if(receiver===2)engine.configureProfile({settings});
        else await batch.command({command:'configure',settings});
      }
    } else if(kind==='color')engine.configureProfile({settings:{background:quantizeQColorChannels(Float32Array.from(values[0]).reverse())}});
    else if(kind==='save')await profiles.command({command:'save',name:values[0]});
    else if(kind==='load')await profiles.command({command:'select',name:values[0]});
    else if(kind==='batch')await batch.command({command:values[0]?'enter':'leave'});
    else if(kind==='add-files')await batch.command({command:'add',filenames:values[0].map(name=>local.get(name))});
    else if(kind==='export-format')await batch.command({command:'configure',exportFormat:values[0]});
    else if(kind==='export-folder')await batch.command({command:'configure',exportDirectory:directory});
    else if(kind==='reset-model') {
      const priorBatch=batch.snapshot();
      await profiles.command({command:'reset'});await sources.command({command:'reset-history'});
      await batch.command({command:'reset-source'});engine.resetProcessing();
      assert.equal(engine.hasSource,false);assert.equal(engine.state(),null);assert.equal(engine.continuing,false);
      assert.equal(batch.snapshot().queue.exportDirectory,priorBatch.queue.exportDirectory);
      assert.deepEqual(batch.snapshot().exportOptions,priorBatch.exportOptions);
    } else assert.ok(['snapshot','document','persist'].includes(kind),kind);
    const expected=reference.expected[0][index];
    if(expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'selected controls at '+index);
      assert.deepEqual(batch.selectedProfile(),profileFromFrame(expected.batch),'retained batch controls at '+index);
      assert.deepEqual(profiles.names(),expected.profiles.map(entry=>entry.name),'saved list at '+index);
      const encoded=Buffer.from(expected.paletteRuntime.backupArgbBits,'hex');
      const backup=Array.from({length:encoded.length/4},(_,i)=>encoded.readFloatLE((Math.floor(i/4)*4+3-i%4)*4));
      assert.deepEqual(engine.paletteRuntime(),{backup,customized:expected.paletteRuntime.customized},'palette revert lifetime at '+index);
      assert.deepEqual(batch.snapshot().queue.queue.map(entry=>({...entry,filename:[...local].find(([,target])=>target===entry.filename)[0]})),expected.queue);
      assert.equal(batch.snapshot().queue.processingIndex,expected.processingIndex);
      assert.equal(batch.snapshot().queue.exportFormat,expected.exportFormat);
    }
    if(kind==='document') {
      compareDocumentState(engine.snapshot().document,expected.document.state,parameters);
      assert.equal(engine.snapshot().committedCount,Buffer.from(expected.document.state.settingsBits,'hex').readInt32LE(8));
    }
  }
});

test('batch source reset rejects an active workspace before changing controls or files',async t=>{
  const {batch}=await context(t);await batch.command({command:'configure',settings:{advanced:true,colorCount:9,smoothness:12}});
  await batch.command({command:'enter'});const before=batch.snapshot();
  await assert.rejects(batch.command({command:'reset-source'}),/Return to single-image/);
  assert.deepEqual(batch.snapshot(),before);
});

test('batch source reset clears a completed processing cursor and keeps its exported file',async t=>{
  const {directory,batch}=await context(t),filename=join(directory,'image.bmp'),second=join(directory,'second.bmp'),output=join(directory,'output');
  const bytes=Buffer.from(Object.values(references[0].inputs)[0],'base64');
  await mkdir(output);await writeFile(filename,bytes);await writeFile(second,bytes);
  await batch.command({command:'add',filenames:[filename,second]});
  await batch.command({command:'configure',exportFormat:'svg',exportDirectory:output});
  await batch.command({command:'start'});await batch.completion;
  assert.equal(batch.snapshot().job.status,'complete');assert.equal(batch.snapshot().queue.processingIndex,1);
  const saved=readFileSync(join(output,'image.svg'));
  await batch.command({command:'leave'});await batch.command({command:'reset-source'});
  assert.equal(batch.snapshot().queue.processingIndex,0);assert.deepEqual(batch.snapshot().queue.queue,[]);
  assert.deepEqual(batch.snapshot().thumbnails,[]);assert.equal(batch.snapshot().queue.exportDirectory,output);
  assert.deepEqual(readFileSync(join(output,'image.svg')),saved);
});
