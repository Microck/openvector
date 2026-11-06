import test from 'node:test';
import assert from 'node:assert/strict';
import {AutoUpdate, readAutoUpdatePreferences, wizardPageSettings} from '../auto-update.mjs';

test('auto-update validates and owns preference choices',()=>{
  const choices={palette:true,segmentation:false,smoothing:true};
  const result=readAutoUpdatePreferences(choices); choices.palette=false;
  assert.equal(result.palette,true);
  assert.throws(()=>readAutoUpdatePreferences({...choices,extra:true}));
  assert.throws(()=>readAutoUpdatePreferences({...choices,palette:1}));
});
test('palette changes debounce to one update and disabled pages do not fire',()=>{
  const auto=new AutoUpdate();
  auto.enterPage('palette',{paletteMode:0,palette:[]},{},0);
  assert.equal(auto.nextDeadline(),500);
  auto.settingsChanged(300);
  assert.equal(auto.nextDeadline(),800);
  assert.deepEqual(auto.advance(799),[]);
  assert.deepEqual(auto.advance(800),['palette']);
  assert.deepEqual(auto.advance(900),[]);
  auto.settingsChanged(1000);
  auto.enterPage('other',{}, {},1100);
  assert.deepEqual(auto.advance(1500),[]);
});
test('unchecked pending work resumes after the user enables it',()=>{
  const auto=new AutoUpdate({palette:false,segmentation:true,smoothing:true});
  auto.enterPage('palette',{paletteMode:0,palette:[]},{},0);
  assert.equal(auto.nextDeadline(),null);
  auto.click('palette',100);
  assert.deepEqual(auto.advance(599),[]);
  assert.deepEqual(auto.advance(600),['palette']);
});
test('advanced palette mode normalizes without mutating caller settings',()=>{
  const input={advanced:true,paletteMode:0};
  assert.equal(wizardPageSettings('palette',input).paletteMode,1);
  assert.equal(input.paletteMode,0);
  assert.equal(wizardPageSettings('other',input),input);
});
