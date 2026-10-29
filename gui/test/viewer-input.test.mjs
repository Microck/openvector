import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {ViewerInput,browserViewerKey,browserViewerPointer} from '../viewer-input.mjs';
import {browserViewerWheel,viewerDirectScale,viewerExportScale,viewerFit,viewerScalePosition,viewerSlider,viewerWheel} from '../viewer-zoom.mjs';
import {decodeImage} from '../../src/image-input.mjs';
import {initialViewerView,viewerImagePoint,viewerPan,viewerScroll,viewerResize} from '../viewer-geometry.mjs';

const [, ...references] = gunzipSync(readFileSync(new URL('../../test/fixtures/viewer-input-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

for (const reference of references) test('native viewer key dispatch: '+reference.id,() => {
  const input = new ViewerInput();input.setLayer(2);input.setTool(7);
  for (const [index,action] of reference.actions.entries()) {
    const expected = reference.expectedFrames[index],label = reference.id+' action '+index;
    if (action.action === 25) {
      // Qt flips its modifier-key bit when exposing QKeyEvent::modifiers().
      // This is declared-event construction, never an expected-state input.
      const modifiers = action.modifiers ^ (action.key === 0x01000020 ? 0x02000000 : 0);
      assert.equal(modifiers,expected.keyModifiers,label+' actual Qt producer');
      input.key({phase:action.phaseKey,key:action.key,modifiers},{bitmapEnabled:true,vectorEnabled:true});
    } else if (action.action === 6) input.selectTool(action.tool);
    else if (action.action === 10) {input.setLayer(1);input.setTool(0);}
    else if (action.action === 29) {
      assert.equal(action.slot,'setPrimaryLayer(LayerType)');input.setLayer(action.value);
    } else if (action.action === 28) {
      assert.equal(action.slot,'setIsBatchMode(bool)');input.setBatchMode(Boolean(action.value));
    } else assert.ok([26,27,30].includes(action.action));
    const state = input.snapshot();assert.equal(state.tool,expected.inputMode,label+' tool');
    if (expected.viewerInput) {
      const h = expected.viewerInput;
      assert.deepEqual([state.primaryLayer,state.baseLayer,state.tool,Number(state.shiftHeld),state.panRestore,
        Number(state.heldVector),Number(state.heldBitmap),Number(state.blocked)],h.slice(3),label+' view state');
    }
    if (expected.viewerLayers) {
      assert.deepEqual([state.primaryLayer,state.baseLayer,state.currentLayer],expected.viewerLayers.slice(3,6),label+' actual layer');
      assert.deepEqual(expected.viewerLayers.slice(6,9),[1,1,1],label+' enabled layer buttons');
    }
  }
});

test('browser viewer producer uses current DOM modifiers and rejects invalid key commands before mutation',() => {
  const input = new ViewerInput();input.setTool(3);input.setLayer(1);
  const event = browserViewerKey({key:'Shift',shiftKey:true,ctrlKey:false,altKey:false,metaKey:false},'press');
  assert.deepEqual(event,{phase:'press',key:0x01000020,modifiers:0x02000000});
  const before = input.snapshot();
  for (const invalid of [{...event,phase:'unknown'},{...event,key:NaN},{...event,modifiers:-1}]) {
    assert.throws(()=>input.key(invalid,{bitmapEnabled:true,vectorEnabled:true}),/Viewer keys/);
    assert.deepEqual(input.snapshot(),before);
  }
  input.key(event,{bitmapEnabled:true,vectorEnabled:true});assert.equal(input.snapshot().tool,0);
  input.key(browserViewerKey({key:'Shift',shiftKey:false,ctrlKey:false,altKey:false,metaKey:false},'release'),
    {bitmapEnabled:true,vectorEnabled:true});assert.equal(input.snapshot().tool,3);
});

test('captured Chromium button chords produce each native mouse phase once',()=>{
  const reference=JSON.parse(readFileSync(new URL('../../test/fixtures/browser-pointer-chords.json',import.meta.url),'utf8'));
  assert.ok(reference.events.every(event=>event.isTrusted && event.pointerType==='mouse'));
  assert.deepEqual(reference.events.map(browserViewerPointer),[
    {phase:'move',buttons:0,button:0,modifiers:0},
    {phase:'press',buttons:2,button:2,modifiers:0},
    {phase:'press',buttons:3,button:1,modifiers:0},
    {phase:'release',buttons:2,button:1,modifiers:0},
    {phase:'release',buttons:0,button:2,modifiers:0},
  ]);
});

test('browser pointer metadata preserves middle/extra buttons and current modifiers',()=>{
  const modifiers={shiftKey:true,ctrlKey:true,altKey:true,metaKey:true};
  assert.deepEqual(browserViewerPointer({button:1,buttons:5,...modifiers}),
    {phase:'press',button:4,buttons:5,modifiers:0x1e000000});
  assert.deepEqual(browserViewerPointer({button:2,buttons:1,...modifiers}),
    {phase:'release',button:2,buttons:1,modifiers:0x1e000000});
  assert.deepEqual(browserViewerPointer({button:3,buttons:9}),{phase:'press',button:8,buttons:9,modifiers:0});
  assert.deepEqual(browserViewerPointer({button:4,buttons:1}),{phase:'release',button:16,buttons:1,modifiers:0});
});

const zoomRows=gunzipSync(readFileSync(new URL('../../test/fixtures/viewer-zoom-spans-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const zoomScales=zoomRows.filter(row=>row.kind==='scale');
const fromBits=bits=>Buffer.from(bits,'hex').readDoubleLE();
const adjacent=(bits,step)=>{
  const bytes=Buffer.from(bits,'hex');bytes.writeBigUInt64LE(bytes.readBigUInt64LE()+BigInt(step));return bytes.readDoubleLE();
};
const initialView=initialViewerView;

test('viewer slider and export getter retain all 301 separate native results',()=>{
  assert.equal(zoomScales.length,301);
  for(const row of zoomScales) {
    const before=initialView(),next=viewerSlider(before,row.position);
    assert.equal(next.slider,row.position);
    assert.equal(next.zoom,Math.max(0.2,Math.min(24,fromBits(row.producerBitsLE))),String(row.position));
    assert.equal(viewerExportScale(next),fromBits(row.getterBitsLE),String(row.position));
    assert.equal(viewerScalePosition(fromBits(row.getterBitsLE)),row.inverseGetter,String(row.position));
    assert.deepEqual(before,initialView());
  }
});

test('viewer inverse admits exact native binary64 transition boundaries and neighbors',()=>{
  const transitions=zoomRows.filter(row=>row.kind==='threshold');assert.equal(transitions.length,300);
  for(const row of transitions) {
    if(row.bitsLE==='0000000000000000')assert.ok(viewerScalePosition(24)<row.position);
    else {
      assert.equal(viewerScalePosition(adjacent(row.bitsLE,-1)),row.below,row.position+' below');
      assert.equal(viewerScalePosition(fromBits(row.bitsLE)),row.at,row.position+' at');
      assert.equal(viewerScalePosition(adjacent(row.bitsLE,1)),row.above,row.position+' above');
    }
  }
});

test('viewer inverse matches 4099 native samples and the scale boundary',()=>{
  const samples=zoomRows.filter(row=>row.kind==='inverse');assert.equal(samples.length,4099);
  for(const row of samples)assert.equal(viewerScalePosition(fromBits(row.bitsLE)),row.position,row.bitsLE);
  assert.equal(viewerScalePosition(-10),0);assert.equal(viewerScalePosition(100),viewerScalePosition(24));
  for(const value of [NaN,Infinity,-Infinity,undefined,'1'])assert.throws(()=>viewerScalePosition(value),/finite/);
});

test('fit and 100 percent retain native displayed scale separately from export view scale',()=>{
  const references=gunzipSync(readFileSync(new URL('../../test/fixtures/viewer-metrics-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  for(const reference of references) {
    const before=initialView();let view=before;
    if(reference.id!=='cold') {
      const filename=reference.id==='wide'?'factory-reset-inputs/alpha/image.bmp':'bitmap-controls-inputs/tall.bmp';
      const image=decodeImage(readFileSync(new URL('../../evidence/'+filename,import.meta.url)));
      const [left,top,right,bottom]=reference.view.rectangle,extent=reference.view.scrollbarExtent;
      view=viewerFit(view,image,{width:right-left+1-extent,height:bottom-top+1-extent});
    }
    assert.equal(view.zoom,reference.view.matrix[0],reference.id+' transform');
    assert.equal(view.slider,reference.owners[0].slider.value,reference.id+' slider');
    assert.equal(viewerExportScale(view),reference.owners[0].viewScale,reference.id+' getter');
    assert.deepEqual(before,initialView());
  }
  const one=viewerDirectScale({...initialView(),zoom:2,slider:144},1);
  assert.equal(one.zoom,1);assert.equal(one.slider,100);
  assert.equal(viewerExportScale(one),references[0].owners[0].viewScale);
  assert.deepEqual(one.center,{x:0,y:0});
  assert.equal(viewerFit(initialView(),{width:1,height:1},{width:0,height:0}).zoom,0.2);
});

const centerRows=gunzipSync(readFileSync(new URL('../../test/fixtures/viewer-center-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

function nativeFit(reference) {
  const filename=reference.id==='wide'?'factory-reset-inputs/alpha/image.bmp':'bitmap-controls-inputs/tall.bmp';
  const image=decodeImage(readFileSync(new URL('../../evidence/'+filename,import.meta.url)));
  const [left,top,right,bottom]=reference.geometry.viewportRectangle;
  return viewerFit(initialView(),image,{width:right-left+1,height:bottom-top+1});
}

test('image fit reproduces native scene bounds, scrollbars and distinct retained centers',()=>{
  for(const reference of centerRows.filter(row=>row.id!=='cold')) {
    const view=nativeFit(reference),native=reference.geometry;
    assert.equal(view.zoom,reference.view.matrix[0]);
    assert.deepEqual(view.bars,native.bars,reference.id+' bars');
    assert.deepEqual(Object.values(view.scene),native.scene,reference.id+' scene');
    assert.deepEqual(Object.values(view.center),reference.view.center,reference.id+' retained center');
    assert.deepEqual(Object.values(view.qtCenter),native.qtCenter,reference.id+' requested center');
    assert.deepEqual(Object.values(view.indent),native.indents,reference.id+' indents');
    // The native origin is an integer translation, whose zero has no sign bit.
    assert.deepEqual(view.origin,{x:0-native.bars[0].value,y:0-native.bars[1].value});
    const mapped=viewerImagePoint(view,{x:Math.trunc(view.area.width/2+0.5),y:Math.trunc(view.area.height/2+0.5)});
    assert.deepEqual(Object.values(mapped),native.mapped,reference.id+' owned inverse map');
  }
});

test('native integer scrolling clips drags and uses the current scrollbar bounds',()=>{
  const view=nativeFit(centerRows.find(row=>row.id==='wide')),before=structuredClone(view);
  const moved=viewerPan(view,{x:1000,y:-1000});
  assert.equal(moved.bars[0].value,moved.bars[0].minimum);
  assert.equal(moved.bars[1].value,moved.bars[1].maximum);
  assert.deepEqual(view,before);
  assert.equal(viewerScroll(moved,'horizontal',-2147483648),moved);
  assert.throws(()=>viewerScroll(view,'wrong',0),/scrollbar/);
  assert.throws(()=>viewerScroll(view,'horizontal',0.5),/integer/);
  assert.deepEqual(view,before);
  const resized=viewerResize(view,{width:134,height:132});
  assert.deepEqual(resized.area,{width:134,height:132});
  assert.equal(resized.retainCenterOnResize,false);
});

test('browser wheel uses angle units and slider steps retain native integer division',()=>{
  assert.equal(browserViewerWheel({wheelDeltaY:120,deltaY:-100}),120);
  assert.throws(()=>browserViewerWheel({deltaY:-100}),/native wheel angle/);
  const point={x:0,y:0};
  for(const [angle,position] of [[120,110],[-120,90],[11,100],[-11,100],[0,100],[2147483647,300],[-2147483648,0]]) {
    const view=viewerWheel(initialView(),angle,point),native=zoomScales[position];
    assert.equal(view.slider,position);
    assert.equal(view.zoom,Math.max(0.2,Math.min(24,fromBits(native.producerBitsLE))));
  }
  for(const value of [1.5,NaN,2147483648])assert.throws(()=>viewerWheel(initialView(),value,point),/signed integer/);
  assert.throws(()=>viewerSlider(initialView(),1.5),/integer/);
});

test('wheel zoom matches native anchor results and center clipping',()=>{
  const rows=gunzipSync(readFileSync(new URL('../../test/fixtures/viewer-wheel-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const areas=[{width:133,height:133},{width:851,height:619},{width:134,height:132}];
  assert.equal(rows.length,8127);
  for(const row of rows) {
    const area=areas[row.sample];
    const before=viewerSlider({...initialView(),image:{width:20000,height:20000},area,
      center:{x:row.center[0],y:row.center[1]}},row.position);
    const next=viewerWheel(before,row.delta,{x:row.point[0],y:row.point[1]});
    const expected=Buffer.from(row.expectedAnchorBitsLE,'hex');
    assert.equal(next.slider,row.nextPosition);
    assert.equal(next.center.x,Math.max(0,Math.min(20000,expected.readDoubleLE(0))));
    assert.equal(next.center.y,Math.max(0,Math.min(20000,expected.readDoubleLE(8))));
    assert.deepEqual(Object.values(before.center),row.center);
  }
});
