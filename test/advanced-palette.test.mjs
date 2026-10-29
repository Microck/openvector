import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {CoreSession} from '../src/palette.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {cleanStartupProfile,settingsFromBits} from './helpers/native-gui.mjs';

const [, ...references] = gunzipSync(readFileSync(new URL('./fixtures/palette-controls-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [, ...range] = gunzipSync(readFileSync(new URL('./fixtures/advanced-palette-range-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const bits = values => Buffer.from(values.buffer,values.byteOffset,values.byteLength).toString('hex');

for (const reference of [...references.filter(row => /count-(thirteen|twenty-four)$/.test(row.id)),...range])
  test('advanced palette range matches native: '+reference.id,() => {
    const session = new CoreSession(),decoded = decodeImage(Buffer.from(reference.input.bytes,'base64'));
    const metadata = resolveImageFilename(reference.input.name);
    const document = session.createDocument({...decoded,shadowControl:metadata.shadowControl});
    const automatic = document.runAutomatic({...cleanStartupProfile.controls,extension:metadata.extension});
    let settings = {...automatic.settings,advanced:true,paletteMode:1},count,completed = 0;
    for (const action of reference.actions) {
      if (action.slot === 'setAdvNumColors(int)') count = action.value;
      if (action.action !== 32) continue;
      const {state,didWork} = document.runTask('palette',{...settings,lower:count,upper:count});
      const expected = reference.expectedCompleted[completed++].state;
      assert.equal(didWork,true);
      assert.equal(state.randomState,expected.randomState,'retained random stream at '+count);
      assert.deepEqual(Array.from(state.completed),expected.completed,'candidate availability at '+count);
      assert.equal(bits(state.scores),expected.scoreBits,'every cached float32 score at '+count);
      assert.deepEqual(state.palettes.map(bits),expected.paletteBits,'every cached ARGB candidate at '+count);
      assert.deepEqual(state.settings,settingsFromBits(expected.settingsBits,expected.committedPaletteArgbBits),
        'committed controls and preceding selected palette at '+count);
      assert.deepEqual(state.stageKeys,{palette:expected.cacheKey,...expected.stageKeys},'retained processing keys');
      assert.deepEqual(state.fitting.geometry,expected.fittedGeometry,'palette work preserves previous vector');
      const cached = document.runTask('palette',{...settings,lower:count,upper:count});
      assert.equal(cached.didWork,false);assert.deepEqual(cached.state,state,'repeat request preserves the live core');
      // The GUI receives candidate colors through QColor. The next task uses
      // those selected controls, not native expected colors as its inputs.
      const argb = state.palettes[count];
      settings = {...settings,palette:Float32Array.from({length:argb.length},(_,index) =>
        Math.round(argb[Math.floor(index/4)*4+3-index%4]*65535)/65535)};
    }
    assert.equal(completed,reference.expectedCompleted.length);
  });
