import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,mkdir,writeFile,readFile,rm} from 'node:fs/promises';
import {dirname,join,resolve,relative,sep} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {setTimeout as delay} from 'node:timers/promises';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {defaultQuickSavePreferences} from '../quick-save.mjs';
import {QuickSaveFile} from '../quick-save-file.mjs';
import {startGui} from '../server.mjs';
import {BatchQueue} from '../../src/batch-runner.mjs';

const references=gunzipSync(readFileSync(new URL('../../test/fixtures/quick-save-batch-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const cancellations=gunzipSync(readFileSync(new URL('../../test/fixtures/quick-save-cancellation-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const taskKinds={load:8,classification:1,palette:2,segmentation:3,smoothing:4,fitting:5,export:6};
const bitmap=Buffer.from(Object.values(references[0].inputs)[0],'base64');
async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-quick-save-batch-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-quick-save-batch-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}

for(const reference of [...references,...cancellations])test('native Quick Save batch lifetime: '+reference.id,async t=>{
  const directory=await directoryFor(t),inputRoot=resolve(dirname(Object.keys(reference.inputs)[0]),'..');
  const captureWorkingDirectory=resolve(inputRoot,'../..');
  const output=join(directory,'output'),localInputs=join(directory,'inputs');
  await Promise.all(['docs','tools','output'].map(name=>mkdir(join(directory,name))));
  // Paths are capture context, translated only in fixture setup. Native field
  // values and behavior remain the assertions, including unsuccessful loads.
  const remap=filename=>{
    if(filename==='')return '';
    if(filename===reference.seed.classic_quick_save_widget.directory)return join(directory,'docs');
    if(filename===reference.seed.adv_quick_save_widget.directory)return join(directory,'tools');
    if(filename===dirname(captureWorkingDirectory))return dirname(directory);
    assert.ok(filename.startsWith(inputRoot+sep),filename);
    return join(localInputs,relative(inputRoot,filename));
  };
  for(const [filename,encoded] of Object.entries(reference.inputs)) {
    const target=remap(filename);await mkdir(dirname(target),{recursive:true});await writeFile(target,Buffer.from(encoded,'base64'));
  }
  const preferences=defaultQuickSavePreferences(directory);
  for(const [mode,owner] of [['basic','classic_quick_save_widget'],['advanced','adv_quick_save_widget']]) {
    const seed=reference.seed[owner];preferences[mode]={format:seed.extension,directory:remap(seed.directory),
      expanded:reference.seed['VmQt::MainModel::ToggleGroupBox:'+owner]?.expanded==='true'};
  }
  const engine=new GuiEngine({workingDirectory:directory,quickSave:preferences});
  let cancelAfter=null,cancelSent=false,cancelPromise;
  const batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4)),publish:value=>{
      const current=value.job.current;
      if(cancelAfter && !cancelSent && value.job.running && current?.complete && current.index===cancelAfter.index && taskKinds[current.stage]===cancelAfter.task) {
        // The real publisher observes a completed task. Cancel through the
        // ordinary controller before its runner continues to the next stage.
        cancelSent=true;cancelPromise=batch.command({command:'cancel'});
      }
    }});
  for(const [index,[action,...values]] of reference.actions.entries()) {
    if(action==='snapshot') {}
    else if(action==='open-wide')engine.open({name:remap(values[0]),bytes:Buffer.from(reference.inputs[values[0]],'base64')});
    else if(action==='batch')await batch.command({command:values[0]?'enter':'leave'});
    else if(action==='add-files')await batch.command({command:'add',filenames:values[0].map(remap)});
    else if(action==='export-folder')await batch.command({command:'configure',exportDirectory:output});
    else if(action==='export-format')await batch.command({command:'configure',exportFormat:values[0]});
    else if(action==='arm-cancel')cancelAfter={task:values[0],index:values[1]};
    else if(action==='task-trace')assert.equal(cancelSent,true);
    else if(action==='start-batch') {
      await batch.command({command:'start'});await batch.completion;await cancelPromise;
      assert.equal(batch.snapshot().job.status,reference.cancelAfter?'cancelled':'complete');
    }
    else assert.fail('Unknown native action: '+action);
    const expected=reference.expected[index];
    if(expected.quickSaveFields) {
      const widgets=Object.fromEntries([['basic',0x1b4],['advanced',0x1c4]].map(([mode,offset])=>{
        const widget=expected.windowResetState.find(widget=>widget.fieldOffset===offset);
        const filename=expected.quickSaveFields.find(field=>field.fieldOffset===offset).filename;
        return [mode,{format:widget.formats[widget.formatIndex],directory:remap(widget.directory),expanded:widget.expanded,filename}];
      }));
      assert.deepEqual(engine.quickSave(),widgets,'window fields at '+index);
      assert.equal(engine.sourceFilename,remap(expected.sourceHistory.filename),'current source at '+index);
      assert.equal(engine.hasSource,expected.sourceHistory.fromClipboard!==null,'loaded source at '+index);
    }
  }
});

test('batch source protection uses the current source while the Quick Save target retains the old name',async t=>{
  const directory=await directoryFor(t),prior=join(directory,'prior.svg'),current=join(directory,'current.svg'),output=join(directory,'output');
  await mkdir(output);await writeFile(prior,bitmap);await writeFile(current,bitmap);
  const engine=new GuiEngine({workingDirectory:directory});engine.open({name:prior,bytes:bitmap});
  const fields=engine.quickSave(),batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4))});
  await batch.command({command:'add',filenames:[current]});await batch.command({command:'configure',exportFormat:'svg',exportDirectory:output});
  await batch.command({command:'start'});await batch.completion;await batch.command({command:'leave'});
  assert.equal(engine.sourceFilename,current);assert.deepEqual(engine.quickSave(),fields);
  const writer=new QuickSaveFile(engine);
  // The former source is an ordinary existing target now, requiring consent.
  assert.equal((await writer.command({command:'save',kind:'vector',mode:'basic'})).stage,'overwrite');
  await writer.command({command:'decide',accepted:false});assert.deepEqual(await readFile(prior),bitmap);
  await writer.command({command:'save',kind:'vector',mode:'basic'});assert.equal((await writer.command({command:'decide',accepted:true})).status,'saved');
  assert.match(await readFile(prior,'utf8'),/<svg/);
  engine.quickSave({mode:'basic',filename:'current.svg'});
  await assert.rejects(writer.command({command:'save',kind:'vector',mode:'basic'}),/source file/);
  assert.deepEqual(await readFile(current),bitmap);assert.equal(writer.pending,false);
  const before=engine.sourceFilename;assert.throws(()=>engine.open({name:'invalid.bmp',bytes:Buffer.from('invalid')}));assert.equal(engine.sourceFilename,before);
  engine.reset();assert.equal(engine.sourceFilename,'');assert.equal(engine.quickSave().basic.filename,'current');
});

test('HTTP Quick Save retains widget fields after batch and guards the new source path',async t=>{
  const directory=await directoryFor(t),prior=join(directory,'prior.svg'),current=join(directory,'current.svg'),output=join(directory,'output');
  await mkdir(output);await writeFile(prior,bitmap);await writeFile(current,bitmap);
  const host=await startGui({port:0,workingDirectory:directory,profileFilename:join(directory,'profiles.json')});t.after(()=>host.close());
  const command=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{...(input===undefined?{}:{method:'POST',body:JSON.stringify(input)}),
      headers:{Origin:host.url,'Content-Type':'application/json','User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'}});
    const value=await response.json();assert.equal(response.status,status,JSON.stringify(value));return value;
  };
  await command('sources',{command:'open',filename:prior});const fields=await command('quickSave');
  await command('batch',{command:'add',filenames:[current]});await command('batch',{command:'configure',exportFormat:'svg',exportDirectory:output});
  await command('batch',{command:'start'});
  let snapshot,deadline=performance.now()+10000;
  do {snapshot=await command('batch');if(!snapshot.job.running)break;await delay(5);}while(performance.now()<deadline);
  assert.equal(snapshot.job.status,'complete');await command('batch',{command:'leave'});
  assert.deepEqual(await command('quickSave'),fields);
  assert.equal((await command('quickSaveFile',{command:'save',kind:'vector',mode:'basic'})).stage,'overwrite');
  await command('quickSaveFile',{command:'decide',accepted:false});assert.deepEqual(await readFile(prior),bitmap);
  await command('quickSave',{mode:'basic',filename:'current.svg'});
  assert.match((await command('quickSaveFile',{command:'save',kind:'vector',mode:'basic'},400)).error,/source file/);
  assert.deepEqual(await readFile(current),bitmap);
});

for(const decision of ['skip-one','skip-all','cancel'])test('Quick Save source ownership through independent overwrite preflight: '+decision,async t=>{
  const directory=await directoryFor(t),prior=join(directory,'prior.bmp'),first=join(directory,'first.bmp'),last=join(directory,'last.bmp'),output=join(directory,'output');
  await mkdir(output);await writeFile(first,bitmap);await writeFile(last,bitmap);
  const existing=join(output,'first.svg');await writeFile(existing,'owned existing first');
  if(decision==='skip-all')await writeFile(join(output,'last.svg'),'owned existing last');
  const engine=new GuiEngine({workingDirectory:directory});engine.open({name:prior,bytes:bitmap});
  const fields=engine.quickSave(),before=engine.snapshot();
  const batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,cancellation:new Int32Array(new SharedArrayBuffer(4))});
  await batch.command({command:'add',filenames:[first,last]});await batch.command({command:'configure',exportFormat:'svg',exportDirectory:output});
  await batch.command({command:'start'});assert.equal(batch.snapshot().job.status,'overwrite');
  assert.equal(engine.sourceFilename,prior);assert.deepEqual(engine.quickSave(),fields);
  await batch.command({command:'decide',decision:decision==='cancel'?'cancel':'skip'});await batch.completion;
  assert.equal(await readFile(existing,'utf8'),'owned existing first');assert.deepEqual(engine.quickSave(),fields);
  assert.equal(batch.snapshot().job.status,decision==='skip-one'?'complete':decision==='skip-all'?'no-files':'cancelled');
  if(decision==='skip-one') {
    assert.equal(engine.sourceFilename,last);assert.equal(engine.hasSource,true);
    assert.match(await readFile(join(output,'last.svg'),'utf8'),/<svg/);
  } else {
    assert.equal(engine.sourceFilename,prior);assert.deepEqual(engine.snapshot(),before);
    if(decision==='skip-all')assert.equal(await readFile(join(output,'last.svg'),'utf8'),'owned existing last');
  }
});

test('a pre-aborted GUI-engine batch preserves source and Quick Save owners before loading',async t=>{
  const directory=await directoryFor(t),prior=join(directory,'prior.bmp'),next=join(directory,'next.bmp');await writeFile(next,bitmap);
  const engine=new GuiEngine({workingDirectory:directory});engine.open({name:prior,bytes:bitmap});
  const queue=new BatchQueue({applicationDirectory:directory,workingDirectory:directory});queue.addFiles([next]);
  const fields=engine.quickSave(),before=engine.snapshot();
  assert.equal((await engine.runBatch(queue,{signal:AbortSignal.abort()})).summary.status,'cancelled');
  assert.equal(engine.sourceFilename,prior);assert.deepEqual(engine.quickSave(),fields);assert.deepEqual(engine.snapshot(),before);
});
