import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import test from 'node:test';
import {exportSvg} from '../../src/svg.mjs';
import {GuiEngine} from '../engine.mjs';
import {startIsolatedGui} from './gui-host.mjs';

const references=['tiff','tiff-jpeg','jpeg-alpha','tiff-legacy','tiff-predictor'].flatMap(corpus=>gunzipSync(readFileSync(new URL('../../test/fixtures/'+corpus+'-conversion-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse).slice(1));
const alphaInputs=new Set(['background-tiff-load-alpha','background-tiff-jpeg-load-gray-alpha-flat','background-tiff-jpeg-load-gray-alpha-art','background-tiff-predictor-load-alpha32','background-tiff-predictor-load-planar-gray64']);
for (const reference of references) test('GUI native TIFF conversion: '+reference.id,()=>{
  const engine=new GuiEngine(),opened=engine.open({name:reference.input.name,bytes:Buffer.from(reference.input.bytes,'base64')});
  assert.equal(opened.file.format,'tiff');assert.equal(opened.file.hasAlpha,alphaInputs.has(reference.id));
  const state=engine.automatic({settings:opened.settings}),native=reference.expectedAutomatic.state;
  assert.deepEqual(state.stageKeys,{palette:native.cacheKey,...native.stageKeys});
  assert.equal(state.svg,exportSvg({width:state.width,height:state.height,geometry:native.fittedGeometry}));
  assert.equal(state.regionCount,native.segmentation.counts.length);
  assert.deepEqual(engine.export({format:'svg'}).bytes,Buffer.from(state.svg));
});

for (const [corpus,id] of [['tiff','background-tiff-load-tiled'],['tiff-jpeg','background-tiff-jpeg-load-ycbcr-tiled'],['jpeg-alpha','background-tiff-jpeg-load-gray-alpha-art'],['tiff-legacy','background-tiff-legacy-load-next-span'],['tiff-legacy','background-tiff-legacy-load-thunder-retained'],['tiff-predictor','background-tiff-predictor-load-alpha32'],['tiff-predictor','background-tiff-predictor-load-planar-gray64']]) test('GUI '+corpus+' upload uses the independent worker and attachment route',async t=>{
  const host=await startIsolatedGui({port:0});t.after(()=>host.close());
  const reference=references.find(row=>row.id===id);
  const headers={'Content-Type':'application/json',Origin:host.url,'User-Agent':'OpenAI File Downloader, XaiImageApiFetch/1.0'};
  const api=(action,body)=>fetch(host.url+'/api/'+action,{method:'POST',headers,body:JSON.stringify(body)});
  let response=await api('open',{name:reference.input.name,bytes:reference.input.bytes});assert.equal(response.status,200);
  const opened=await response.json();assert.equal(opened.file.format,'tiff');
  response=await api('automatic',{settings:opened.settings});assert.equal(response.status,200);
  const state=await response.json(),native=reference.expectedAutomatic.state;
  assert.equal(state.svg,exportSvg({width:state.width,height:state.height,geometry:native.fittedGeometry}));
  response=await fetch(host.url+'/api/export?format=svg',{headers:{'User-Agent':headers['User-Agent']}});
  assert.equal(response.status,200);assert.ok(response.headers.get('Content-Disposition').includes(reference.input.name.replace(/\.tiff?$/i,'.svg')));
  assert.equal(await response.text(),state.svg);
});
