import test, {after} from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {createReadStream,mkdtempSync,readFileSync,writeFileSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,dirname,basename,resolve,isAbsolute} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createInterface} from 'node:readline';
import {createGunzip,gunzipSync} from 'node:zlib';
import {exportSvg} from '../src/svg.mjs';
import {exportEps} from '../src/eps.mjs';
import {exportPdf} from '../src/pdf.mjs';
import {exportAi} from '../src/ai.mjs';
import {exportDxf} from '../src/dxf.mjs';
import {defaultExportOptions} from '../src/export-options.mjs';
import {decodeImage,resolveImageFilename} from '../src/image-input.mjs';
import {CoreSession} from '../src/settings.mjs';
import {renderBitmap} from '../src/bitmap-raster.mjs';
import {encodeBmp,encodeJpeg,encodePng,encodeTiff} from '../src/bitmap-codecs.mjs';
import {cleanStartupProfile} from './helpers/native-gui.mjs';
import {observedEpsCreationDate,presentationOfNativeGeometry} from './helpers/native-vector-output.mjs';
import {replayApplicationEdits} from './helpers/application-edit-reference.mjs';

const cli=fileURLToPath(new URL('../bin/openvector.mjs',import.meta.url));
const directory=mkdtempSync(join(tmpdir(),'openvector-cli-'));
after(()=>{
  assert.ok(isAbsolute(directory));assert.equal(dirname(directory),resolve(tmpdir()));
  assert.ok(basename(directory).startsWith('openvector-cli-'));
  rmSync(directory,{recursive:true,force:true});
});
const rows=createInterface({input:createReadStream(new URL('./fixtures/document-automatic-reference.ndjson.gz',import.meta.url))
  .pipe(createGunzip()),crlfDelay:Infinity});
let reference;
for await(const line of rows){const row=JSON.parse(line);if(row.id==='native-gui-clean-session'){reference=row;break;}}
const encoded=Buffer.from(reference.inputs['ambivalence-gui-square.bmp'],'base64');
const frame=reference.steps.find(step=>step.action==='auto').expected;
const options={geometry:frame.state.fittedGeometry,width:16,height:16,palette:frame.state.segmentation.palette};
const input=join(directory,'input image.bmp');writeFileSync(input,encoded);
const invoke=(args,options={})=>spawnSync(process.execPath,[cli,...args],{cwd:directory,...options});
const success=result=>{assert.equal(result.status,0,result.stderr.toString());assert.equal(result.stderr.length,0);};
const failure=(result,pattern)=>{assert.equal(result.status,1);assert.equal(result.stdout.length,0);assert.match(result.stderr.toString(),pattern);};

const [, ...editReferences] = gunzipSync(readFileSync(new URL('./fixtures/segmentation-edit-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const editCommand = action => action.action !== 1 ? {action:{2:'undo',3:'redo',4:'revert'}[action.action]}
  : action.kind === 3 ? {tool:'pencil',from:action.coordinates.slice(0,2),to:action.coordinates.slice(2),color:action.argb.slice().reverse(),commit:action.commit}
    : {tool:action.kind === 5 ? 'fill' : 'zap',x:action.x,y:action.y,color:action.argb.slice().reverse(),commit:action.commit};
const [, ...gestureReferences] = gunzipSync(readFileSync(new URL('./fixtures/segmentation-gesture-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const gestureTools = {0:'pan',3:'pencil',4:'picker',5:'fill',6:'zap'};
const [, ...backgroundReferences] = gunzipSync(readFileSync(new URL('./fixtures/background-edit-reference.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);

// Compile only declared inputs. Native output states below are used solely to
// assert the final geometry/visibility and unavailable-vector diagnostics.
function backgroundPlan(reference,sources) {
  const commands = [{background:{action:'begin'}}],tools = {...gestureTools,7:'background'};
  let page = 'background',advanced = false;
  for (const action of reference.actions) {
    if (action.action === 16) continue; // Native diagnostic queries do not mutate output.
    if (action.action === 6 || action.phase) {
      const event = action.action === 6 ? {phase:'select',tool:tools[action.tool]}
        : {phase:action.phase,point:action.point,buttons:action.buttons,button:action.button,modifiers:action.modifiers};
      commands.push(page === 'background' ? {background:{action:'gesture',events:[event]}} : {gesture:event});
    } else if (action.action === 10) { commands.push({action:'begin'});page = 'editing'; }
    else if (action.action === 11) commands.push({gesture:{phase:'color',color:action.argb.slice().reverse()}});
    else if (action.action === 12) { commands.push({background:{action:'begin'}});page = 'background'; }
    else if ([13,14,15].includes(action.action)) commands.push({background:{action:{13:'undo',14:'redo',15:'revert'}[action.action]}});
    else if (action.action === 18) { commands.push({source:sources[action.source]});page = 'export';advanced = false; }
    else if (action.action === 19) {
      const property = {'setUseAdvancedMode(bool)':'advanced','setAdvContourSmoothness(int)':'smoothness','setAdvCurveComplexity(int)':'curveComplexity'}[action.slot];
      assert.ok(property);
      if (property === 'advanced') advanced = Boolean(action.value);
      commands.push({settings:{[property]:property === 'advanced' ? advanced : action.value}});
    } else if (action.action === 21) commands.push({background:{action:'toggle',point:action.point}});
    else if (action.action === 22) {
      const point = action.kind === 3 ? action.coordinates.slice(0,2) : [action.x+.5,action.y+.5];
      commands.push({gesture:{phase:'press',point,buttons:1,button:1}});
    } else if ([2,3,4].includes(action.action)) commands.push({action:{2:'undo',3:'redo',4:'revert'}[action.action]});
    else if (action.action === 5 || action.action === 23) {
      if (page === 'editing') { commands.push({action:advanced ? 'commit' : 'finish'});page = advanced ? 'smoothing' : 'export'; }
      else if (page === 'smoothing') commands.push({action:'process',stage:'smoothing'},{action:'process',stage:'fitting'});
      else { commands.push({background:{action:'finish'}});page = 'export'; }
    } else assert.fail('Unknown native background input '+action.action);
  }
  return commands;
}

for (const reference of backgroundReferences) test('CLI native background recipe: '+reference.id,() => {
  const image = join(directory,reference.id+'-'+reference.input.name),edits = join(directory,reference.id+'.json'),sources = {};
  writeFileSync(image,Buffer.from(reference.input.bytes,'base64'));
  for (const [name,input] of Object.entries(reference.additionalInputs)) {
    sources[name] = reference.id+'-source-'+name;
    writeFileSync(join(directory,sources[name]),Buffer.from(input.bytes,'base64'));
  }
  writeFileSync(edits,JSON.stringify(backgroundPlan(reference,sources)));
  let native = reference.expectedAutomatic.state,completed = 0;
  for (const [index,action] of reference.actions.entries()) {
    if (action.action === 18) native = reference.expectedFrames[index+1].replacedSource.automatic.state;
    if (action.action === 5 || action.action === 23) native = reference.expectedCompleted[completed++].state;
  }
  const output = invoke(['vectorize',image,'--edits',edits,'--layout','stacked']);
  if (native.fittedGeometry === null) {
    failure(output,/Vector paths are unavailable/);
    const protectedFile = join(directory,reference.id+'-protected.svg');writeFileSync(protectedFile,'keep');
    failure(invoke(['vectorize',image,'--edits',edits,'--output',protectedFile,'--force']),/Vector paths are unavailable/);
    assert.equal(readFileSync(protectedFile,'utf8'),'keep');
  } else {
    success(output);
    const hiddenRegions = reference.expectedFrames.at(-1).visibility.visible.flatMap((visible,index) => visible ? [] : [index]);
    const geometry=native.stageKeys.segmentationEdits ? native.fittedGeometry
      : presentationOfNativeGeometry(native.fittedGeometry).geometry;
    assert.deepEqual(output.stdout,Buffer.from(exportSvg({geometry,
      width:native.segmentation.width,height:native.segmentation.height,hiddenRegions})));
  }
});

test('CLI source paths resolve from the plan and remain protected with force',() => {
  const plans = mkdtempSync(join(directory,'nested-source-')),source = join(plans,'next.bmp'),edits = join(plans,'plan.json');
  writeFileSync(source,encoded);writeFileSync(edits,JSON.stringify([{source:'next.bmp'}]));
  success(invoke(['vectorize',input,'--edits',edits]));
  failure(invoke(['vectorize',input,'--edits',edits,'--output',source,'--format','svg','--force']),/different files/);
  assert.deepEqual(readFileSync(source),encoded);assert.deepEqual(readFileSync(input),encoded);
});

test('CLI validates background batches and selected settings before replacing output',() => {
  const edits = join(directory,'bad-background.json'),output = join(directory,'bad-background-protected.svg');
  writeFileSync(output,'keep');
  for (const command of [{background:null},{background:{action:'gesture',events:[]}},
    {background:{action:'toggle',point:[null,1]}},{background:{action:'unknown'}},
    {settings:null},{settings:{smothness:9}},{settings:{smoothness:99}},
    {source:''},{background:{action:'revert'},action:'undo'}]) {
    writeFileSync(edits,JSON.stringify([command]));
    const rejected = invoke(['vectorize',input,'--edits',edits,'--output',output,'--force']);
    assert.equal(rejected.status,1);assert.equal(rejected.stdout.length,0);assert.ok(rejected.stderr.length);
    assert.equal(readFileSync(output,'utf8'),'keep');
  }
});

for (const reference of gestureReferences.filter(row => row.expectedCompleted.length)) test('CLI gesture plan matches independent application geometry: '+reference.id,() => {
  const image = join(directory,'gesture-'+reference.id+'-'+reference.input.name),edits = join(directory,'gesture-'+reference.id+'.json');
  writeFileSync(image,Buffer.from(reference.input.bytes,'base64'));
  const commands = reference.actions.map(action => {
    if (action.action === 6) return {gesture:{phase:'select',tool:gestureTools[action.tool]}};
    if (action.phase) return {gesture:{phase:action.phase,point:action.point,buttons:action.buttons,button:action.button,modifiers:action.modifiers}};
    if (action.action === 11) return {gesture:{phase:'color',color:action.argb.slice().reverse()}};
    return {action:{2:'undo',3:'redo',4:'revert',5:'finish',10:'begin'}[action.action]};
  });
  writeFileSync(edits,JSON.stringify(commands));
  const result = invoke(['vectorize',image,'--edits',edits,'--layout','stacked']);success(result);
  const expected=replayApplicationEdits({input:reference.input,actions:reference.actions},{gesture:true});
  assert.deepEqual(result.stdout,Buffer.from(exportSvg(expected)));
});

for (const reference of editReferences)
  test('CLI edits match independent straight-color application SVG: '+reference.id,() => {
    const image = join(directory,reference.id+'-'+reference.input.name),edits = join(directory,reference.id+'-edits.json');
    writeFileSync(image,Buffer.from(reference.input.bytes,'base64'));
    writeFileSync(edits,JSON.stringify(reference.actions.filter(action => action.action !== 5).map(editCommand)));
    const result = invoke(['vectorize',image,'--edits',edits,'--layout','stacked']);success(result);
    const expected=replayApplicationEdits({input:reference.input,actions:reference.actions.filter(action=>action.action!==5)});
    assert.deepEqual(result.stdout,Buffer.from(exportSvg(expected)));
  });

test('CLI rejects malformed edit plans before writing or replacing output',() => {
  const edits = join(directory,'invalid-edits.json'),output = join(directory,'edited-protected.svg');
  writeFileSync(output,'keep');
  for (const plan of ['{', '{}', '[null]', '[{"action":"unknown"}]', '[{"tool":"fill","x":8,"y":8,"color":[0,0,0,1],"action":"undo"}]']) {
    writeFileSync(edits,plan);
    const rejected = invoke(['vectorize',input,'--edits',edits,'--output',output,'--force']);
    assert.equal(rejected.status,1);assert.equal(rejected.stdout.length,0);assert.ok(rejected.stderr.length);
    assert.equal(readFileSync(output,'utf8'),'keep');
  }
});

test('vectorize defaults to the native application SVG export choices on stdout',()=>{
  const result=invoke(['vectorize',input]);success(result);
  assert.deepEqual(result.stdout,Buffer.from(exportSvg({...options,...defaultExportOptions})));
});

test('vectorize supports binary stdin with explicit filename metadata',()=>{
  const result=invoke(['vectorize','-','--filename','square.bmp','--layout','stacked'],{input:encoded});success(result);
  assert.deepEqual(result.stdout,Buffer.from(exportSvg(options)));
  failure(invoke(['vectorize','-'],{input:encoded}),/--filename/);
  failure(invoke(['vectorize',input,'--filename','square.bmp']),/only.*stdin|only.*-/i);
});


for(const [format,writer] of Object.entries({svg:exportSvg,eps:exportEps,pdf:exportPdf,ai:exportAi,dxf:exportDxf})){
  test(`vectorize writes complete ${format.toUpperCase()} bytes to stdout`,()=>{
    const started = Date.now();
    const args=['vectorize',input,'--format',format,'--layout','stacked'];
    if(format==='dxf')args.push('--dxf-mode','dense-lines');
    const result=invoke(args);success(result);
    let writerOptions = options;
    if (format === 'eps') {
      // EPS records wall-clock seconds. Pin only this field after checking it
      // belongs to the real child invocation; geometry bytes remain exact.
      const creationDate = observedEpsCreationDate(result.stdout,started,Date.now());
      writerOptions = { ...options, creationDate };
    }
    assert.deepEqual(result.stdout,Buffer.from(writer(writerOptions)));
  });
}

test('vectorize defaults to the native application DXF export choices',()=>{
  const result=invoke(['vectorize',input,'--format','dxf']);success(result);
  assert.deepEqual(result.stdout,Buffer.from(exportDxf({...options,...defaultExportOptions})));
});

test('vectorize infers format and supports flags before the input',()=>{
  const output=join(directory,'result file.SVG');
  const result=invoke(['vectorize','--output',output,'--layout','stacked',input]);success(result);
  assert.equal(result.stdout.length,0);assert.deepEqual(readFileSync(output),Buffer.from(exportSvg(options)));
});

test('vectorize requires explicit force before overwriting',()=>{
  const output=join(directory,'existing.svg');writeFileSync(output,'keep');
  failure(invoke(['vectorize',input,'-o',output]),/--force/);
  assert.equal(readFileSync(output,'utf8'),'keep');
  const forced=invoke(['vectorize',input,'-o',output,'--force','--layout','stacked']);success(forced);
  assert.deepEqual(readFileSync(output),Buffer.from(exportSvg(options)));
  failure(invoke(['vectorize',input,'--output',input,'--format','svg','--force']),/different.*input|input.*output/i);
  assert.deepEqual(readFileSync(input),encoded);
});

test('vectorize validates usage and format before processing',()=>{
  failure(invoke(['vectorize']),/image|input/i);
  failure(invoke(['vectorize',input,'--format','webp']),/SVG.*EPS.*PDF.*AI.*DXF/i);
  failure(invoke(['vectorize',input,'--unknown']),/unknown|unexpected/i);
  failure(invoke(['vectorize',input,'--output',join(directory,'unknown.bin')]),/format|extension/i);
  failure(invoke(['vectorize',input,input]),/one|single|image/i);
});

const bitmapScenes=['bitmap-scenes-reference','bitmap-jpeg-scenes-reference','bitmap-visibility-reference'].flatMap(name=>
  gunzipSync(readFileSync(new URL('./fixtures/'+name+'.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse));

// A separate JS tracing/raster implementation supplies the complete expected
// image where the original application's associated-color export was wrong.
// Opaque and binary-alpha captures below remain exact historical byte checks.
function independentBitmap(reference,format,decoded) {
  const {extension,shadowControl}=resolveImageFilename(reference.sourceFilename);
  const document=new CoreSession().createDocument({...decoded,shadowControl});
  document.runAutomatic({...structuredClone(cleanStartupProfile.controls),extension});
  if (reference.actions?.length) {
    document.beginBackgroundEditing();
    for (const [,action,...point] of reference.actions) {
      if (action===0) document.toggleBackground(point);
      else document[{1:'undoBackground',2:'redoBackground',3:'revertBackground'}[action]]();
    }
    document.finishBackgroundEditing();
  }
  const state=document.snapshot(),background=document.backgroundState();
  const hasAlpha=background.undo.length!==0 ||
    (format==='png'&&document.sourceAlphaType()!==0&&![1,2].includes(state.settings.backgroundMode));
  const rendered=renderBitmap({geometry:presentationOfNativeGeometry(state.fitting.geometry).geometry,
    width:state.original.width,height:state.original.height,scale:reference.scale,hasAlpha,
    hiddenRegions:background.hiddenRegions});
  if (hasAlpha&&['png','tif','tiff'].includes(format)) for (let i=0;i<rendered.bgra.length;i+=4)
    for (let c=0;c<3;c++) rendered.bgra[i+c]=rendered.bgra[i+3] ? Math.floor(rendered.bgra[i+c]*255/rendered.bgra[i+3]) : 0;
  return {bmp:encodeBmp,jpg:encodeJpeg,png:encodePng,tif:encodeTiff}[format](rendered);
}
for (const reference of bitmapScenes) test('bitmap CLI preserves reference drawing with straight colors: '+reference.id,()=>{
  const source=Buffer.from(reference.sourceBytes,'base64'),decoded=decodeImage(source);
  const translucent=decoded.bgra.some((byte,index)=>index%4===3&&byte!==0&&byte!==255);
  const args=['vectorize','-','--filename',reference.sourceFilename,'--scale',String(reference.scale)];
  if (reference.actions?.length) {
    const plan=join(directory,reference.id+' bitmap edits.json');
    const commands=[{background:{action:'begin'}}];
    for (const [,action,...point] of reference.actions) commands.push({background:action===0?
      {action:'toggle',point}:{action:['toggle','undo','redo','revert'][action]}});
    writeFileSync(plan,JSON.stringify(commands));args.push('--edits',plan);
  }
  for (const [filename,expected] of Object.entries(reference.expected)) {
    const format=filename.slice(filename.lastIndexOf('.')+1);
    const converted=invoke([...args,'--format',format],{input:source});success(converted);
    const bytes=translucent ? independentBitmap(reference,format,decoded) : Buffer.from(expected.bytes,'base64');
    assert.deepEqual(converted.stdout,Buffer.from(bytes),reference.id+' '+filename);
  }
});

test('bitmap CLI infers aliases, preserves files and rejects invalid flags before processing',()=>{
  const reference=bitmapScenes.find(row=>row.id==='opaque-four-plugins');
  const source=join(directory,'bitmap reference.bmp');writeFileSync(source,Buffer.from(reference.sourceBytes,'base64'));
  for (const [extension,canonical] of [['BMP','bmp'],['JPEG','jpg'],['PNG','png'],['TIFF','tif']]) {
    const native=bitmapScenes.find(row=>row.id===reference.id && Object.hasOwn(row.expected,'result.'+canonical));
    const target=join(directory,'bitmap result.'+extension),expected=Buffer.from(native.expected['result.'+canonical].bytes,'base64');
    const converted=invoke(['vectorize',source,'-o',target]);success(converted);
    assert.equal(converted.stdout.length,0);assert.deepEqual(readFileSync(target),expected);
    failure(invoke(['vectorize',source,'-o',target]),/--force/);
    assert.deepEqual(readFileSync(target),expected);
    success(invoke(['vectorize',source,'-o',target,'--force']));
    const aliased=invoke(['vectorize',source,'--format',extension.toLowerCase()]);success(aliased);
    assert.deepEqual(aliased.stdout,expected);
  }
  for (const scale of ['0','-1','NaN','Infinity','nope'])
    failure(invoke(['vectorize','missing.bmp','--format','png','--scale='+scale]),/positive finite/);
  failure(invoke(['vectorize','missing.bmp','--format','svg','--scale','2']),/only.*bitmap/);
  failure(invoke(['vectorize','missing.bmp','--format','svg','--flatten']),/only.*non-PNG/);
  failure(invoke(['vectorize','missing.bmp','--format','png','--flatten']),/only.*non-PNG/);
  failure(invoke(['vectorize',source,'-o',source,'--format','png','--force']),/different files/);
  assert.deepEqual(readFileSync(source),Buffer.from(reference.sourceBytes,'base64'));
});

test('bitmap CLI makes flatten consent explicit without partial output or replacement',()=>{
  const reference=bitmapScenes.find(row=>row.id==='alpha-png'),source=Buffer.from(reference.sourceBytes,'base64');
  for (const format of ['bmp','jpg','tif']) {
    const args=['vectorize','-','--filename',reference.sourceFilename,'--format',format];
    failure(invoke(args,{input:source}),/--flatten.*--format png/);
    const target=join(directory,'flatten decision.'+format);writeFileSync(target,'keep');
    failure(invoke([...args,'--output',target,'--force'],{input:source}),/--flatten/);
    assert.equal(readFileSync(target,'utf8'),'keep');
    const converted=invoke([...args,'--flatten'],{input:source});success(converted);
    assert.ok(decodeImage(converted.stdout).bgra.every((channel,index)=>index%4!==3 || channel===255));
  }
});

test('conversion errors never replace an existing output or emit partial stdout',()=>{
  const invalid=join(directory,'invalid.png'),output=join(directory,'protected.svg');
  writeFileSync(invalid,'not an image');writeFileSync(output,'keep');
  failure(invoke(['vectorize',invalid,'--output',output,'--force']),/Unsupported image format/);
  assert.equal(readFileSync(output,'utf8'),'keep');
  const missing=join(directory,'failed.svg');
  failure(invoke(['vectorize',invalid,'--output',missing]),/Unsupported image format/);
  assert.equal(existsSync(missing),false);
});

test('help takes precedence and package version is discoverable',()=>{
  for(const args of [['vectorize','missing.png','--unknown','--help'],['help','vectorize']]){
    const result=invoke(args);success(result);assert.match(result.stdout.toString(),/Usage: openvector vectorize/);
  }
  const version=invoke(['--version']);success(version);
  const metadata=JSON.parse(readFileSync(new URL('../package.json',import.meta.url),'utf8'));
  assert.equal(version.stdout.toString(),metadata.version+'\n');
});

for (const codec of ['jpeg','gif','tiff','tiff-jpeg','jpeg-alpha','bmp-expanded','tiff-legacy','tiff-predictor']) test(codec.toUpperCase()+' binary stdin CLI exports all five formats from each native fitted geometry',()=>{
  const [, ...conversions]=gunzipSync(readFileSync(new URL('./fixtures/'+codec+'-conversion-reference.ndjson.gz',import.meta.url)))
    .toString().trim().split('\n').map(JSON.parse);
  const writers={svg:exportSvg,eps:exportEps,pdf:exportPdf,ai:exportAi,dxf:exportDxf};
  for (const reference of conversions) for (const [format,writer] of Object.entries(writers)) {
    const started=Date.now();
    const args=['vectorize','-','--filename',reference.input.name,'--format',format,'--layout','stacked'];
    if(format==='dxf')args.push('--dxf-mode','dense-lines');
    const converted=invoke(args,
      {input:Buffer.from(reference.input.bytes,'base64')});success(converted);
    const finished=Date.now();
    const native=reference.expectedAutomatic.state;
    const expected=writer({width:native.sourceDimensions[0],height:native.sourceDimensions[1],
      ...presentationOfNativeGeometry(native.fittedGeometry),
      ...(format==='eps' ? {creationDate:observedEpsCreationDate(converted.stdout,started,finished)} : {})});
    assert.deepEqual(converted.stdout,Buffer.from(expected),reference.id+' '+format+' native geometry export');
  }
});
