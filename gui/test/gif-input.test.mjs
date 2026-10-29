import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {exportSvg} from '../../src/svg.mjs';
import {GuiEngine} from '../engine.mjs';
import {startIsolatedGui} from './gui-host.mjs';

const [, ...references]=gunzipSync(readFileSync(new URL('../../test/fixtures/gif-conversion-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
for (const reference of references) test('GUI native GIF conversion: '+reference.id,()=>{
  const engine=new GuiEngine(),opened=engine.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  assert.equal(opened.file.format,'gif');assert.equal(opened.file.hasAlpha,reference.id.endsWith('transparent'));
  const state=engine.automatic({settings:opened.settings}),native=reference.expectedAutomatic.state;
  assert.deepEqual(state.stageKeys,{palette:native.cacheKey,...native.stageKeys});
  assert.equal(state.svg,exportSvg({width:state.width,height:state.height,geometry:native.fittedGeometry}));
  assert.equal(state.regionCount,native.segmentation.counts.length);
  assert.deepEqual(engine.export({format:'svg'}).bytes,Buffer.from(state.svg));
});

test('GUI animated GIF upload processes the original first frame through its worker and attachment route',async t=>{
  const host=await startIsolatedGui({port:0});t.after(()=>host.close());
  const reference=references.find(row=>row.id.endsWith('animated'));
  const headers={'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const api=(action,body)=>fetch(host.url+'/api/'+action,{method:'POST',headers,body:JSON.stringify(body)});
  let response=await api('open',{name:reference.input.name,bytes:reference.input.bytes});assert.equal(response.status,200);
  const opened=await response.json();assert.equal(opened.file.format,'gif');
  response=await api('automatic',{settings:opened.settings});assert.equal(response.status,200);
  const state=await response.json(),native=reference.expectedAutomatic.state;
  assert.equal(state.svg,exportSvg({width:state.width,height:state.height,geometry:native.fittedGeometry}));
  response=await fetch(host.url+'/api/export?format=svg',{headers:{'User-Agent':headers['User-Agent']}});
  assert.equal(response.status,200);assert.match(response.headers.get('Content-Disposition'),/gif-gui-animated.svg/);
  assert.equal(await response.text(),state.svg);
});
