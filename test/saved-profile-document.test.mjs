import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {CoreSession} from '../src/settings.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {SavedAdvancedProfiles,applyAdvancedProfile,defaultAdvancedProfile,runAdvancedProfile} from '../src/saved-profiles.mjs';
import {cleanStartupProfile,compareDocumentState,settingsFromBits} from './helpers/native-gui.mjs';
import {CancelTaskException} from '../src/image-document.mjs';

const [provenance,...references] = gunzipSync(readFileSync(new URL('./fixtures/profile-document-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const settingsSlots = {'setUseAdvancedMode(bool)':'advanced','setPaletteMode(int)':'paletteMode',
  'setAdvSegmentationComplexity(int)':'complexity','setAdvContourSmoothness(int)':'smoothness'};

for (const reference of references) test('native retained profile document: '+reference.id,() => {
  const session = new CoreSession(),profiles = new SavedAdvancedProfiles();
  let document,batchMode = false;
  let selected = {settings:structuredClone(cleanStartupProfile.controls),colorCount:2,basicColorCount:0,
    paletteColorCount:0,recommendedBackgroundMode:0};
  for (let index=0;index<reference.actions.length;index++) {
    const [kind,...values] = reference.actions[index];
    if (kind === 'automatic') {
      const source = values[0],filename = resolveImageFilename(source),decoded = decodeImage(Buffer.from(reference.inputs[source],'base64'));
      if (document) {
        document.setSource({...decoded,shadowControl:filename.shadowControl});
        selected.settings = {...selected.settings,palette:new Float32Array(),backgroundMode:-1,background:Float32Array.of(1,1,1,0)};
      } else document = session.createDocument({...decoded,shadowControl:filename.shadowControl});
      const result = document.runAutomatic({...selected.settings,extension:filename.extension});
      selected.settings = result.settings;
      if (result.paletteCount !== null) selected.colorCount = selected.paletteColorCount = result.paletteCount;
      selected.recommendedBackgroundMode = result.recommendedBackgroundMode;
    } else if (kind === 'set') {
      const [receiver,signature,value] = values;
      if (receiver === 1) continue; // Requested batch controls do not commit the active document.
      if (signature === 'setAdvNumColors(int)') selected.colorCount = value;
      else {
        const name = settingsSlots[signature];assert.ok(name,signature);
        selected.settings[name] = typeof selected.settings[name] === 'boolean' ? Boolean(value) : value;
      }
    } else if (kind === 'save') profiles.put(values[0],selected);
    else if (kind === 'batch') batchMode = values[0];
    else if (kind === 'load') {
      const choice = profiles.select(values[0]);
      if (choice.profile && !batchMode) {
        // MainModel copies the stored controls into its document before it
        // forces the separately selected GUI settings into advanced mode.
        document.commitSettings(choice.profile.settings);
        selected = applyAdvancedProfile(null,choice.profile);
      }
    } else assert.equal(kind,'document');
    if (kind === 'document') {
      const expected = reference.expected[index].document;
      compareDocumentState(document.snapshot(),expected.state,provenance.parameters);
      assert.deepEqual(selected.settings,settingsFromBits(expected.controls.selectedSettingsBits,expected.selectedPaletteArgbBits),
        'selected controls remain distinct from committed document controls');
    }
  }
});

test('shared advanced replay preserves complete native cold/cache-hit documents and owns observer snapshots',()=>{
  const [provenance,...recordings]=gunzipSync(readFileSync(new URL('./fixtures/profile-replay-window-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const reference=recordings.find(row=>row.id==='default-fast-forward-cache-hit'),source=reference.actions[0][1];
  const decoded=decodeImage(Buffer.from(reference.inputs[source],'base64')),filename=resolveImageFilename(source);
  const document=new CoreSession().createDocument({...decoded,shadowControl:filename.shadowControl}),profile=defaultAdvancedProfile();
  const runtime={backup:Float32Array.of(.2,.3,.4,1),customized:true},before=structuredClone({profile,runtime});
  const expected=reference.expected.filter(frame=>frame.document),didWork=[];
  for (let repeat=0;repeat<2;repeat++) {
    document.commitSettings(profile.settings);
    const stages=[],replay=runAdvancedProfile(document,profile,{paletteRuntime:runtime,onStage:(stage,snapshot)=>{
      stages.push(stage);snapshot.profile.settings.background.fill(1);snapshot.paletteRuntime.backup.fill(0);
      if(snapshot.state.working)snapshot.state.working.bgra.fill(0);
    }});
    assert.deepEqual(stages,['palette','segmentation','smoothing','fitting']);
    compareDocumentState(document.snapshot(),expected[repeat].document.state,provenance.parameters);
    assert.deepEqual(replay.state,document.snapshot());assert.deepEqual({profile,runtime},before);
    assert.equal(replay.profile.colorCount,2);assert.equal(replay.paletteRuntime.customized,false);
    didWork.push(replay.didWork);
  }
  assert.deepEqual(didWork[1],{palette:false,segmentation:false,smoothing:false,fitting:false});
});

for (const target of ['palette','segmentation','smoothing','fitting']) test(`advanced replay stops on ${target} cancellation and retries its retained document`,()=>{
  const [,...recordings]=gunzipSync(readFileSync(new URL('./fixtures/profile-replay-window-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const reference=recordings.find(row=>row.id==='default-fast-forward-cache-hit'),source=reference.actions[0][1];
  const decoded=decodeImage(Buffer.from(reference.inputs[source],'base64')),filename=resolveImageFilename(source);
  const document=new CoreSession().createDocument({...decoded,shadowControl:filename.shadowControl}),profile=defaultAdvancedProfile();
  document.commitSettings(profile.settings);
  const completed=[],observed=[];
  assert.throws(()=>runAdvancedProfile(document,profile,{
    onProgress:event=>{observed.push(event.stage);if(event.stage===target)document.requestCancel();},
    onStage:stage=>completed.push(stage),
  }),CancelTaskException);
  assert.ok(observed.includes(target),'the requested checkpoint ran');
  assert.deepEqual(completed,['palette','segmentation','smoothing','fitting'].slice(0,['palette','segmentation','smoothing','fitting'].indexOf(target)),
    'cancelled and later stages must not publish completion');
  assert.deepEqual(document.snapshot().original.bgra,decoded.bgra,'source survives cancellation');
  const retried=runAdvancedProfile(document,profile);
  assert.equal(retried.state.status,0);assert.ok(retried.state.stageKeys.fitting.length);
});
