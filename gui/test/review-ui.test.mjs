import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {dirname,join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {completedResultPage,resultBackPage,readReviewPreferences} from '../review-state.mjs';
import {profileFromFrame} from '../../test/helpers/native-gui.mjs';
import {startGui} from '../server.mjs';
import {readProcessingPreferences} from '../processing-preferences.mjs';

const references=['review-visibility','review-back'].flatMap(name=>
  gunzipSync(readFileSync(new URL(`../../test/fixtures/${name}-reference.ndjson.gz`,import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));
const inputs=JSON.parse(readFileSync(new URL('../../test/fixtures/review-actions-inputs.json',import.meta.url),'utf8'));
const pages={review:19,export:18,colors:10,palette:11,background:15};
const selectedSlots={'setImageCategory(int)':'category','setPaletteMode(int)':'paletteMode','setUseAdvancedMode(bool)':'advanced'};

for(const reference of references)test('native review visibility/navigation: '+reference.id,()=>{
  const engine=new GuiEngine(),{skipReview}=readReviewPreferences({skipReview:reference.skipReview});let page=0;
  for(const [index,[kind,...values]] of reference.phases[0].entries()) {
    const expected=reference.expected[0][index],label=reference.id+' action '+index;
    if(kind==='automatic') {
      engine.open({name:values[0],bytes:Buffer.from(inputs[values[0]],'base64')});
      const document=engine.automatic({settings:engine.selectedProfile().settings});page=pages[completedResultPage(document.settings,skipReview)];
    } else if(kind==='next') {
      if(page===19)page=18;
      else if(page===15)page=engine.selectedProfile().settings.advanced ? 17 : 19;
      else assert.equal(page,18,'export has no next action');
    } else if(kind==='set') {
      const [receiver,slot,value]=values;
      if(receiver===2) {
        const field=selectedSlots[slot];assert.ok(field,slot);engine.select({settings:{[field]:field==='advanced'?Boolean(value):value}});
      } else if(slot==='setWizardPage(WizardPageIdType)') {assert.equal(value,17);page=17;}
      else if(slot==='goToTroubleShootingPage()')page=19;
      else if(slot==='processBackPress()') {
        if(page===11)page=10;
        else page=pages[resultBackPage(page===19?'review':'export',engine.selectedProfile().settings,skipReview)];
      } else assert.fail(slot);
    } else if(kind==='review-action') {
      assert.equal(values[0],8);engine.review({action:'background'});page=15;
    } else assert.ok(['show','snapshot'].includes(kind));
    assert.equal(page,expected.processing.wizardPage,label+' page');
    if(expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),label+' selected controls');
      assert.equal(nextSkip(expected),skipReview,label+' window preference');
    }
  }
});

function nextSkip(frame) {return frame.windowResetTargets.find(item=>item.fieldOffset===0x14c).checked;}

async function directoryFor(t) {
  const directory=await mkdtemp(join(tmpdir(),'openvector-review-ui-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-review-ui-')));
    await rm(directory,{recursive:true,force:true});
  });return directory;
}

test('review preference HTTP validation is atomic and clean restart retains the shared checkbox',async t=>{
  const directory=await directoryFor(t),profileFilename=join(directory,'profiles.json');
  let host=await startGui({port:0,workingDirectory:directory,profileFilename});t.after(()=>host?.close());
  const headers={'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const get=async()=>{
    const response=await fetch(host.url+'/api/reviewPreferences',{headers});assert.equal(response.status,200);return response.json();
  };
  const post=async(action,input,status=200)=>{
    const response=await fetch(host.url+'/api/'+action,{method:'POST',headers:{...headers,Origin:host.url,'Content-Type':'application/json'},body:JSON.stringify(input)});
    assert.equal(response.status,status);return response.json();
  };
  assert.deepEqual(await get(),{skipReview:false});
  assert.deepEqual(await post('reviewPreferences',{skipReview:true}),{skipReview:true});
  for(const invalid of [null,[],true,{},{skipReview:1},{skipReview:false,extra:true}]) {
    await post('reviewPreferences',invalid,400);assert.deepEqual(await get(),{skipReview:true});
  }
  await post('batch',{command:'enter'});await post('resetApplicationModel',null);assert.deepEqual(await get(),{skipReview:true});
  await post('reviewPreferences',{skipReview:false});await post('reviewPreferences',{skipReview:true});
  await post('batch',{command:'leave'});
  const filename=join(directory,'processing-preferences.json');await assert.rejects(readFile(filename),{code:'ENOENT'});
  await host.close();host=null;assert.equal((await readProcessingPreferences(filename)).skipReview,true);
  host=await startGui({port:0,workingDirectory:directory,profileFilename});assert.deepEqual(await get(),{skipReview:true});
  await host.close();host=null;
  const stored=JSON.parse(await readFile(filename,'utf8'));
  for(const value of [0,'true',null]) {
    await writeFile(filename,JSON.stringify({...stored,skipReview:value}));await assert.rejects(readProcessingPreferences(filename),/Boolean skipReview/);
  }
  delete stored.skipReview;await writeFile(filename,JSON.stringify(stored));await assert.rejects(readProcessingPreferences(filename),/skipReview.*Choose a new preferencesFilename/);
});
