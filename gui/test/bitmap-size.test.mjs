import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import {decodeImage} from '../../src/image-input.mjs';
import {BitmapSize,readBitmapSizeText,bitmapScaleButtons,bitmapScaleMore} from '../bitmap-size.mjs';
import {createNumericText} from '../numeric-text.mjs';
import {GuiEngine} from '../engine.mjs';
import {defaultQuickSavePreferences,bitmapSaveFormats} from '../quick-save.mjs';
import {dirname,resolve} from 'node:path';

const rows=name=>gunzipSync(readFileSync(new URL('../../test/fixtures/'+name+'.ndjson.gz',import.meta.url)))
  .toString().trim().split('\n').map(JSON.parse);
const [localeContext]=rows('qt-locale-reference');
const symbols=Object.fromEntries(['decimal','group','zero','negative','exponent'].map(name=>
  [name,String.fromCharCode(localeContext.locales.default[name])]));
const numeric=createNumericText({...symbols,positive:'+'});
const fields=pane=>[String(pane.width),String(pane.height),pane.percent];

test('bitmap preset catalog matches every read-only native capture including its menu title',()=>{
  for(const reference of rows('bitmap-scale-reference'))for(const owner of reference.owners) {
    assert.deepEqual(bitmapScaleButtons,owner.buttons);
    assert.deepEqual(owner.menuActions,['More',...bitmapScaleMore.map(value=>'x'+value)]);
  }
});

test('preset multipliers and view scales use the shared scale path without percent conversion',()=>{
  const sizes=new BitmapSize();sizes.sourceChanged({width:16,height:12});
  for(const [value,width,height] of [[1,16,12],[2,32,24],[4,64,48],[5,80,60],[6,96,72],
    [8,128,96],[10,160,120],[12,192,144],[16,256,192],[20,320,240],[24,384,288]]) {
    const state=sizes.command({command:'preset',mode:'basic',value});
    assert.equal(state.scale,value);assert.deepEqual(fields(state.basic),[String(width),String(height),value+'00.0']);
    assert.deepEqual(fields(state.advanced),['16','12','100.0']);
  }
  const state=sizes.command({command:'view-scale',mode:'advanced',value:0.3333333333333333});
  assert.equal(state.scale,0.3333333333333333);assert.deepEqual(fields(state.advanced),['5','4','33.3']);
  assert.deepEqual(fields(state.basic),['384','288','2400.0']);
  for(const input of [{command:'preset',mode:'basic',value:3},{command:'preset',mode:'basic',value:1,extra:true},
    {command:'view-scale',mode:'basic',value:0},{command:'view-scale',mode:'basic',value:Infinity}]) {
    assert.throws(()=>sizes.command(input));assert.deepEqual(sizes.snapshot(),state);
  }
});

for(const reference of rows('bitmap-scale-reference'))test('view-scale command preserves observed getter bits: '+reference.id,()=>{
  const sizes=new BitmapSize();
  const source={wide:'evidence/factory-reset-inputs/alpha/image.bmp',tall:'evidence/bitmap-controls-inputs/tall.bmp'}[reference.id];
  if(source)sizes.sourceChanged(decodeImage(readFileSync(new URL('../../'+source,import.meta.url))));
  const peer=sizes.snapshot().advanced,observed=reference.owners[0];
  const state=sizes.command({command:'view-scale',mode:'basic',value:observed.viewScale});
  const bits=Buffer.alloc(8);bits.writeDoubleLE(state.scale);assert.equal(bits.toString('hex'),observed.viewScaleBitsLE);
  assert.deepEqual(fields(state.basic),{cold:['148','247','98.6'],wide:['132','99','823.9'],tall:['73','133','428.3']}[reference.id]);
  assert.deepEqual(state.advanced,peer);
});

const preferenceInputs=JSON.parse(readFileSync(new URL('../../test/fixtures/review-actions-inputs.json',import.meta.url),'utf8'));
const captureRoot=resolve(dirname(Object.keys(preferenceInputs)[0]),'../../..');
for(const reference of rows('window-export-preferences-reference'))test('native bitmap window choices: '+reference.id,()=>{
  const workingDirectory=reference.workingDirectory??captureRoot;
  const preferences=defaultQuickSavePreferences(workingDirectory,'bitmap');
  // Translate only recorded Qt input into the product schema. Expected native
  // choices are read separately below and never construct the owner.
  for(const [mode,owner] of [['basic','bitmap_size_widget'],['advanced','adv_bitmap_size_widget']]) {
    const seed=reference.seed[owner]??{},format=seed.extension?.toLowerCase();
    if(bitmapSaveFormats.includes(format))preferences[mode].format=format;
    preferences[mode].directory=seed.directory??'';
    preferences[mode].expanded=reference.seed['VmQt::MainModel::ToggleGroupBox:'+owner]?.expanded==='true';
  }
  preferences.scale=Number(reference.seed.adv_bitmap_size_widget?.scale??1);
  const engine=new GuiEngine({workingDirectory,bitmapSave:preferences});
  for(const [index,[action,...values]] of reference.phases[0].entries()) {
    if(action==='open-wide')engine.open({name:values[0],bytes:Buffer.from(preferenceInputs[values[0]],'base64')});
    else if(action==='reset-application-model')engine.resetProcessing();
    else assert.equal(action,'snapshot');
    const widgets=reference.expected[0][index].windowResetState;
    const expected=Object.fromEntries([['basic',476],['advanced',480]].map(([mode,offset])=>{
      const widget=widgets.find(widget=>widget.fieldOffset===offset);
      return [mode,{format:widget.formats[widget.formatIndex],directory:widget.directory,expanded:widget.expanded}];
    }));
    expected.scale=widgets.find(widget=>widget.fieldOffset===480).scale;
    assert.deepEqual(engine.bitmapSavePreferences(),expected,reference.id+' action '+index);
  }
});

for(const reference of rows('bitmap-controls-reference'))test('native bitmap size presentation: '+reference.id,()=>{
  const sizes=new BitmapSize();
  for(const [index,action] of reference.actions.entries()) {
    if(action[0]==='automatic') {
      // Re-home only the recorded fixture path. Input bytes remain the exact
      // owned bitmap captured by the oracle, independent of expected fields.
      const parts=action[1].replaceAll('\\','/').split('/').filter(Boolean),start=parts.indexOf('evidence');
      assert.notEqual(start,-1);
      sizes.sourceChanged(decodeImage(readFileSync(new URL('../../'+parts.slice(start).join('/'),import.meta.url))));
    } else if(action[0]==='bitmap-scale')sizes.setScale(action[1]);
    else assert.equal(action[0],'bitmap-controls');
    const frame=reference.expected[index],snapshot=sizes.snapshot();
    for(const [owner,mode] of ['basic','advanced'].entries()) {
      const expected=frame.bitmapControls[owner].controls.filter(control=>control.kind===1 && control.name==='units_textfield').map(control=>control.text);
      assert.deepEqual(fields(snapshot[mode]),expected,mode+' fields at '+index);
      assert.equal(snapshot.scale,frame.windowResetState.find(widget=>widget.fieldOffset===[476,480][owner]).scale);
    }
  }
});

test('size commits retain peer fields while sharing the settled scale',()=>{
  const sizes=new BitmapSize();sizes.sourceChanged({width:16,height:12});
  let state=sizes.command({command:'commit',mode:'basic',field:'width',value:25});
  assert.equal(state.scale,25/16);assert.deepEqual(fields(state.basic),['25','19','156.2']);
  assert.deepEqual(fields(state.advanced),['16','12','100.0']);
  state=sizes.command({command:'commit',mode:'advanced',field:'height',value:24});
  assert.equal(state.scale,2);assert.deepEqual(fields(state.advanced),['32','24','200.0']);
  assert.deepEqual(fields(state.basic),['25','19','156.2']);
  const retained=sizes.snapshot();assert.deepEqual(sizes.sourceChanged(null),retained);
  state=sizes.sourceChanged({width:10,height:20});
  assert.deepEqual(fields(state.basic),['20','40','200.0']);assert.deepEqual(state.basic,state.advanced);
});

test('size corrections honor both bounds and snap buttons use native small/large increments',()=>{
  const sizes=new BitmapSize();sizes.sourceChanged({width:16,height:12});
  let state=sizes.command({command:'step',mode:'basic',field:'width',direction:1,size:'small'});
  assert.deepEqual(fields(state.basic),['20','15','125.0']);
  state=sizes.command({command:'step',mode:'basic',field:'width',direction:1,size:'large'});
  assert.deepEqual(fields(state.basic),['25','19','156.2']);
  state=sizes.command({command:'step',mode:'basic',field:'width',direction:-1,size:'small'});
  assert.deepEqual(fields(state.basic),['20','15','125.0']);
  state=sizes.command({command:'commit',mode:'basic',field:'height',value:20000});
  assert.equal(state.scale,625);assert.deepEqual(fields(state.basic),['10000','7500','62500.0']);
  state=sizes.command({command:'commit',mode:'basic',field:'width',value:0});
  assert.deepEqual(fields(state.basic),['1','1','6.2']);
});

test('size text producer uses native parse-failure values and percentage suffix rules',()=>{
  const sizes=new BitmapSize();sizes.sourceChanged({width:16,height:12});
  for(const [field,text,expected] of [
    ['percent','125%',['20','15','125.0']],['percent','150,0%',['24','18','150.0']],
    ['width','1.0',['1','1','6.2']],['percent','125% ',['1','1','6.2']],
    ['percent','nan',['1','1','6.2']],['percent','inf',['1','1','6.2']],
    ['percent','-inf',['1','1','6.2']],['percent','-100',['1','1','6.2']],
  ]) {
    const input=readBitmapSizeText({command:'commit',mode:'basic',field,text},numeric);
    assert.deepEqual(fields(sizes.command(input).basic),expected,field+' '+text);
  }
});

test('invalid and cycling size commands preserve the last complete state',()=>{
  const sizes=new BitmapSize(),before=sizes.snapshot();
  assert.throws(()=>sizes.sourceChanged({width:30000,height:1}),/cannot settle/);
  assert.deepEqual(sizes.snapshot(),before);
  for(const input of [null,{command:'commit',mode:'basic',field:'width',value:1.5},
    {command:'commit',mode:'wrong',field:'width',value:20},{command:'step',mode:'basic',field:'scale',direction:1,size:'small'}]) {
    assert.throws(()=>sizes.command(input));assert.deepEqual(sizes.snapshot(),before);
  }
  const snapshot=sizes.snapshot();snapshot.original.width=999;snapshot.basic.width=123;
  assert.deepEqual(sizes.snapshot(),before);
});
