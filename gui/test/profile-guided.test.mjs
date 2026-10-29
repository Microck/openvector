import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mkdtemp,rm} from 'node:fs/promises';
import {join,dirname,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {gunzipSync} from 'node:zlib';
import {GuiEngine} from '../engine.mjs';
import {BatchController} from '../batch-controller.mjs';
import {ProfileController} from '../profile-controller.mjs';
import {AutoUpdate,advancedControlsMatch,advancedFields,pageNeedsUpdate,wizardPageSettings} from '../auto-update.mjs';
import {suggestProfileName} from '../profile-name.mjs';
import {compareDocumentState,profileFromFrame} from '../../test/helpers/native-gui.mjs';

const [provenance,...guidedReferences]=gunzipSync(readFileSync(new URL('../../test/fixtures/profile-guided-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [historyProvenance,...historyReferences]=gunzipSync(readFileSync(new URL('../../test/fixtures/profile-history-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
assert.deepEqual(historyProvenance,provenance);
const [backgroundProvenance,...backgroundReferences]=gunzipSync(readFileSync(new URL('../../test/fixtures/profile-background-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
assert.deepEqual(backgroundProvenance,provenance);
const [modalProvenance,...modalReferences]=gunzipSync(readFileSync(new URL('../../test/fixtures/profile-modal-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
assert.deepEqual(modalProvenance,provenance);
const references=[...guidedReferences,...historyReferences,...backgroundReferences,...modalReferences];
const pages={load:0,mode:4,transparency:6,palette:12,segmentation:13,editing:14,background:15,smoothing:16,export:17};
const slots={'setAdvNumColors(int)':'colorCount','setPaletteMode(int)':'paletteMode','setFlattenImageMode(int)':'backgroundMode',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness'};

// This negative recording invokes a hidden control's model slot. It establishes
// an admission limit, not GUI conformance for a user action exposed on Export.
test('native Play on Export remains an explicit unavailable-control observation',()=>{
  const reference=references.find(row=>row.id==='guided-cached-to-export');assert.ok(reference);
  const playback=reference.actions.findIndex(action=>action[0]==='replay' && action[1]==='play');
  for (const frame of reference.expected.slice(playback)) {
    assert.equal(frame.processing.wizardPage,17);
    if(frame.replay){assert.equal(frame.replay.continuing,true);assert.deepEqual(frame.replay.tasks,[]);}
  }
});

for (const reference of references.filter(row=>row.id!=='guided-cached-to-export'))
test('guided saved-profile workflow matches native: '+reference.id,async t=>{
  const directory=await mkdtemp(join(tmpdir(),'openvector-profile-guided-'));
  t.after(async()=>{
    assert.equal(dirname(directory),resolve(tmpdir()));assert.ok(directory.startsWith(join(tmpdir(),'openvector-profile-guided-')));
    await rm(directory,{recursive:true,force:true});
  });
  const engine=new GuiEngine(),batch=new BatchController({engine,applicationDirectory:directory,workingDirectory:directory,
    cancellation:new Int32Array(new SharedArrayBuffer(4))});
  const profiles=await ProfileController.open({engine,batch,filename:join(directory,'profiles.json')}),automatic=new AutoUpdate();
  let page='load',now=0,name=suggestProfileName(profiles.names()),hasAlpha=false;
  const warnings=[];
  const changePage=next=>{
    if(page==='palette' && next!==page)engine.leavePalette();
    page=next;
    const profile=engine.selectedProfile(),settings=wizardPageSettings(page,profile.settings);
    if(settings!==profile.settings)engine.configureProfile({settings});
    automatic.enterPage(settings.advanced?page:null,settings,engine.snapshot().document.stageKeys,now);
  };
  const update=stage=>{
    const profile=engine.selectedProfile(),count=profile.colorCount;
    if(stage==='palette' && engine.paletteRuntime().customized ||
      stage==='segmentation' && engine.state().segmentationHistory.undoCount>0) {
      warnings.push(stage);return;
    }
    if(stage==='palette') {
      const next=engine.palettes({settings:profile.settings,count,lower:count,upper:count});
      engine.palette({action:'replace',palette:next.palettes.find(candidate=>candidate.count===count).colors.flat()});
    } else if(stage==='segmentation')engine.process({settings:profile.settings,count,through:'segmentation'});
    else engine.vectors({settings:profile.settings,count});
  };
  const next=()=>{
    const profile=engine.selectedProfile(),snapshot=engine.snapshot(),document=snapshot.document;
    const match=advancedControlsMatch(profile.settings,profile.colorCount,document.settings,snapshot.committedCount);
    if(page==='load')changePage('mode');
    else if(page==='transparency')changePage('palette');
    else if(page==='palette') {
      if(!match && profile.settings.paletteMode===1 || pageNeedsUpdate(page,profile.settings,document.stageKeys))update('palette');
      else changePage('segmentation');
    } else if(page==='segmentation') {
      if(!match || pageNeedsUpdate(page,profile.settings,document.stageKeys))update('segmentation');
      else {engine.edit({action:'begin'});changePage('editing');}
    } else if(page==='editing') {engine.edit({action:'finish',settings:profile.settings});changePage('smoothing');}
    else if(page==='smoothing') {
      if(!match || pageNeedsUpdate(page,profile.settings,document.stageKeys))update('smoothing');
      else {engine.background({action:'begin'});changePage('background');}
    } else if(page==='background') {engine.background({action:'finish'});changePage('export');}
  };
  const back=()=>{
    if(page==='background') {engine.background({action:'finish'});changePage('smoothing');return;}
    if(page==='export') {engine.background({action:'begin'});changePage('background');return;}
    const profile=engine.selectedProfile(),snapshot=engine.snapshot(),document=snapshot.document;
    const dirty=['palette','segmentation','smoothing'].includes(page) &&
      ((!advancedControlsMatch(profile.settings,profile.colorCount,document.settings,snapshot.committedCount) &&
        (page!=='palette' || profile.settings.paletteMode===1)) || pageNeedsUpdate(page,profile.settings,document.stageKeys));
    const historyChanged=page==='editing' && engine.state().editor.key!==document.stageKeys.segmentationEdits;
    if(dirty || historyChanged) {
      const restored={...profile.settings};for(const key of advancedFields)restored[key]=document.settings[key];
      engine.palette({action:'count',count:snapshot.committedCount});engine.configureProfile({settings:restored});
      engine.edit({action:'restore'});automatic.settingsChanged(now);return;
    }
    if(page==='editing') {engine.edit({action:'leave'});changePage('segmentation');return;}
    if(page==='smoothing') {engine.edit({action:'begin'});changePage('editing');return;}
    const previous=['mode',...(hasAlpha?['transparency']:[]),'palette','segmentation'];
    changePage(previous[Math.max(0,previous.indexOf(page)-1)]);
  };
  for (const [index,action] of reference.actions.entries()) {
    const [kind,...values]=action,expected=reference.expected[index];
    if(kind==='open') {
      const opened=engine.open({name:values[0],bytes:Buffer.from(reference.inputs[values[0]],'base64')});
      hasAlpha=opened.file.hasAlpha;
      changePage('mode');name=suggestProfileName(profiles.names());
    } else if(kind==='load')await profiles.command({command:'select',name:values[0]});
    else if(kind==='set') {
      const [receiver,signature,value]=values;
      if(receiver===0) {
        if(signature==='processBackPress()')back();
        else if(signature==='advUpdatePalette()')update('palette');
        else if(signature==='advUpdateSegmentation()')update('segmentation');
        else {
          const command={'undoMakeBackground()':'undo','redoMakeBackground()':'redo','revertMakeBackground()':'revert'}[signature];
          assert.ok(command,signature);engine.background({action:command});
        }
      }
      else if(receiver===2 && signature==='addColor()')engine.palette({action:'add'});
      else {
        assert.equal(receiver,2);const key=slots[signature];assert.ok(key,signature);
        engine.configureProfile(key==='colorCount'?{colorCount:value}:{settings:{[key]:value}});
        if(engine.selectedProfile().settings.advanced)automatic.settingsChanged(now);
      }
    } else if(kind==='edit') {
      const [tool,x,y,coordinates,argb,commit]=values;assert.equal(tool,5);assert.deepEqual(coordinates,[0,0,0,0]);
      engine.edit({action:'apply',command:{tool:'fill',x,y,color:argb.toReversed(),commit}});
    } else if(kind==='hide')engine.background({action:'toggle',point:values});
    else if(kind==='white')engine.configureProfile({settings:{background:Float32Array.of(1,1,1,1)}});
    else if(kind==='save')await profiles.command({command:'save',name:values[0],force:true});
    else if(kind==='replay') {
      const reply=await profiles.command({command:values[0],name:values[1]});
      if(reply.replay.page!==null)changePage(reply.replay.page);
    } else if(kind==='next')next();
    else if(kind==='wait') {now+=values[0];for(const stage of automatic.advance(now))update(stage);}
    else if(kind==='start-over') {
      if(page==='load') {engine.reset();name=suggestProfileName(profiles.names());}
      else if(page==='background')engine.background({action:'finish'});
      changePage('load');
    } else assert.ok(['show','snapshot','document'].includes(kind),kind);
    if(index===reference.actions.length-1 && reference.modal?.delay) {
      now+=reference.modal.delay.minimumMilliseconds;
      for(const stage of automatic.advance(now))update(stage);
    }
    assert.equal(pages[page],expected.processing.wizardPage,'wizard page at '+index);
    if(expected.selected) {
      assert.deepEqual(engine.selectedProfile(),profileFromFrame(expected.selected),'selected profile at '+index);
      assert.deepEqual(profiles.names(),expected.profiles.map(row=>row.name),'MRU order at '+index);
      assert.equal(engine.continuing,expected.replay.continuing,'continuing at '+index);
      assert.equal(name,expected.suggestedName,'name lifetime at '+index);
      const runtime=expected.paletteRuntime,bytes=Buffer.from(runtime.backupArgbBits,'hex');
      const backup=Array.from({length:bytes.length/16},(_,color)=>Array.from({length:4},(_,channel)=>bytes.readFloatLE(color*16+(3-channel)*4))).flat();
      assert.deepEqual(engine.paletteRuntime(),{backup,customized:runtime.customized},'palette runtime at '+index);
    }
    if(kind==='document') {
      compareDocumentState(engine.snapshot().document,expected.document.state,provenance.parameters);
      assert.equal(engine.snapshot().committedCount,Buffer.from(expected.document.state.settingsBits,'hex').readInt32LE(8));
    }
    if(expected.history) {
      const actual=engine.state(),{editor,undo,redo}=expected.history;
      assert.deepEqual(actual?.segmentationHistory??{undoCount:0,redoCount:0},
        {undoCount:undo.length,redoCount:redo.length},'history counts at '+index);
      if(editor) {
        assert.ok(actual);
        assert.equal(actual.editor?.key??actual.stageKeys.segmentationEdits,editor.key,'serialized edit key at '+index);
        const native=Buffer.from(editor.bgra,'hex');
        if(editor.header[3]===0 || editor.header[4]===0) {
          assert.deepEqual(editor.header.slice(3,6),[0,0,0],'native empty QImage at '+index);
          assert.equal(native.length,0);assert.equal(actual.segments,null,'no displayed segmentation after source replacement');
        } else {
          assert.deepEqual([actual.segments.width,actual.segments.height],editor.header.slice(3,5),'editor dimensions at '+index);
          const rgba=Buffer.from(actual.segments.rgba,'base64');
          for(let pixel=0;pixel<native.length;pixel+=4) {
            assert.equal(native[pixel+3],255,'opaque corpus permits exact display-pixel comparison');
            assert.deepEqual(Array.from(rgba.subarray(pixel,pixel+4)),
              [native[pixel+2],native[pixel+1],native[pixel],native[pixel+3]],'editor pixel '+pixel+' at '+index);
          }
        }
      }
    }
    if(expected.backgroundHistory) {
      const actual=engine.snapshot().background;
      const native=expected.backgroundHistory;
      assert.equal(actual.count,native.count,'visibility count at '+index);
      assert.deepEqual(actual.hiddenRegions,native.visible.flatMap((visible,region)=>visible?[]:[region]),'hidden regions at '+index);
      assert.deepEqual(actual.undo,native.undo,'background undo at '+index);
      assert.deepEqual(actual.redo,native.redo,'background redo at '+index);
      if(native.active!==null)assert.equal(actual.active,native.active,'background active at '+index);
    }
  }
  if(Object.hasOwn(reference,'modal')) {
    const modal=reference.modal;
    assert.equal(warnings.length,modal?.delay?.activeModalIdentities??(modal?1:0),'pending warning count');
    assert.equal(reference.completedActions,reference.actions.length-(modal?1:0));
    assert.equal(reference.pendingStateObservations,modal?2:0);
    assert.equal(reference.expected.length,reference.actions.length+(modal?1:0));
    if(modal) {
      assert.ok(warnings.every(stage=>stage===page));
      assert.equal(modal.wizardPage,pages[page]);
      const pending=reference.expected.at(-1);
      assert.equal(pending.action,29,'aborted action leaves a separate read-only document observation');
      compareDocumentState(engine.snapshot().document,pending.document.state,provenance.parameters);
      if(modal.autoUpdate)assert.deepEqual(Object.values(automatic.snapshot()).map((widget,index)=>
        [[372,400,432][index],Number(widget.checked),Number(widget.enabled),Number(widget.pending),Number(widget.active),500,1]),
        modal.autoUpdate.widgets.map(widget=>widget.fields),'timer state while warnings remain unanswered');
    }
  }
});
