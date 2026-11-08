import assert from 'node:assert/strict';

/** EPS uses local wall-clock seconds. Validate the emitted field against
 * every real second in the child invocation, including clock-boundary and
 * repeated daylight-saving hours, before using it in an exact byte comparison. */
export function observedEpsCreationDate(bytes,started,finished) {
  const creationDate=bytes.toString().match(/^%%CreationDate: ([^\r\n]+)\r?$/m)?.[1];
  assert.equal(typeof creationDate,'string','EPS creation date is present');
  let matched=false;
  for (let second=Math.floor(started/1000);second<=Math.floor(finished/1000);second++) {
    // Date.toString uses English weekday/month names and local clock fields;
    // ctime moves the year to the end and space-pads a one-digit month day.
    const [weekday,month,day,year,time]=new Date(second*1000).toString().split(' ');
    const expected=`${weekday} ${month} ${String(Number(day)).padStart(2,' ')} ${time} ${year}`;
    if (creationDate===expected) {matched=true;break;}
  }
  assert.ok(matched,'EPS creation date belongs to the real child invocation: '+creationDate);
  return creationDate;
}

/** Change only captured branding metadata. Keep file length, offsets, geometry,
 * and cancellation output intact for the differential comparisons. */
export function publicVectorFixture(bytes) {
  const output = Buffer.from(bytes);
  if (output.subarray(0, 5).toString('ascii') === '%PDF-') {
    const original = Buffer.from('/Producer(VectorMagic.com)');
    const offset = output.indexOf(original);
    if (offset !== -1) output.write('/Producer(OpenVector)     ', offset, 'ascii');
  } else if (output.length >= 236 && output.readUInt32LE(0) === 1 &&
    output.readUInt32LE(4) === 236 && output.toString('ascii', 40, 44) === ' EMF') {
    assert.equal(output.readUInt32LE(60), 64, 'captured EMF description length');
    assert.equal(output.readUInt32LE(64), 108, 'captured EMF description offset');
    output.fill(0, 108, 236);
    output.write('OpenVector\0OpenVector\0\0', 108, 'utf16le');
  }
  return output;
}

// Reference-model projection, not a call to the Rust application under test.
// SVG paint is straight sRGB with separate opacity. Keep the captured geometry
// unchanged while deriving its application-facing colors from associated bytes.
// https://www.w3.org/TR/SVG11/painting.html#SpecifyingPaint
function straightColor(bgra) {
  const alpha=bgra[3];
  return [...bgra.slice(0,3).map(component=>alpha===0 ? 0 : Math.min(255,Math.round(component*255/alpha))),alpha];
}

export function presentationOfNativeGeometry(geometry) {
  const projected=structuredClone(geometry);
  for (const region of projected.regions) region.bgra=straightColor(region.bgra);
  const colors=new Map(projected.regions.map(region=>[region.bgra.join(','),region.bgra]));
  return {geometry:projected,palette:[...colors.values()].flatMap(color=>color.map(byte=>Math.fround(byte/255)))};
}

/** Captured grouped SVG retains the exact alpha byte in each group identifier.
 * Change only associated group/fill/stroke colors; every path and opacity byte
 * remains an exact comparison with the independent captured file. */
export function applicationSvgFixture(bytes) {
  const alphas=[];
  const rgb=(color,alpha)=>{
    const rgba=[0,2,4].map(index=>Number.parseInt(color.slice(index,index+2),16));
    return straightColor([...rgba,alpha]).slice(0,3).map(value=>value.toString(16).padStart(2,'0')).join('');
  };
  const text=bytes.toString().replace(/<g id="#([0-9a-f]{6})([0-9a-f]{2})">|<\/g>|((?:fill|stroke)="#)([0-9a-f]{6})(")/g,
    (token,color,alpha,prefix,paint,suffix)=>{
      if (color!==undefined) {
        alphas.push(Number.parseInt(alpha,16));
        return `<g id="#${rgb(color,alphas.at(-1))}${alpha}">`;
      }
      if (token==='</g>') {assert.ok(alphas.length>0,'balanced captured SVG groups');alphas.pop();return token;}
      return alphas.length ? prefix+rgb(paint,alphas.at(-1))+suffix : token;
    });
  assert.equal(alphas.length,0,'balanced captured SVG groups');
  return Buffer.from(text);
}

/** Application DXF may omit unused internal palette layers. Compare every
 * drawing field and used layer, while independently validating object handles;
 * allocation numbers and unused layer declarations are not drawing geometry. */
export function assertDxfDrawing(actual,expected,label) {
  const parse=bytes=>{
    const lines=bytes.toString().split(/\r?\n/);
    if (lines.at(-1)==='') lines.pop();
    assert.equal(lines.length%2,0,'complete DXF fields');
    const fields=[];
    for (let i=0;i<lines.length;i+=2) fields.push([Number(lines[i].trim()),lines[i+1]]);
    assert.deepEqual(fields.at(-1),[0,'EOF'],'complete DXF file');
    const handles=new Set(),references=[];
    const handle=value=>{assert.match(value,/^[0-9a-f]+$/i,'hexadecimal DXF handle');return BigInt('0x'+value).toString(16);};
    const referenceCode=code=>(code>=330&&code<=369)||(code>=390&&code<=399)||(code>=480&&code<=481)||code===1005;
    let section='';
    for (let i=0;i<fields.length;i++) {
      const [code,value]=fields[i];
      if (code===0&&value==='SECTION') section=fields[i+1][1];
      if (code===0&&value==='ENDSEC') section='';
      if ((code===5||code===105)&&section!=='HEADER') {
        const id=handle(value);assert.notEqual(id,'0','non-null DXF object handle');
        assert.ok(!handles.has(id),'unique DXF object handles');handles.add(id);
      }
      if (referenceCode(code)&&handle(value)!=='0') references.push(handle(value));
    }
    for (const handle of references) assert.ok(handles.has(handle),'DXF reference resolves: '+handle);
    const records=[],preamble=[];
    for (const field of fields) {
      if (field[0]===0) records.push([]);
      if (records.length===0) preamble.push(field);
      else records.at(-1).push(field);
    }
    const entities=[],layers=new Map(),metadata=[];
    let layerCount;section='';
    for (const original of records) {
      let record=original;
      const kind=record[0][1];
      if (kind==='SECTION') {
        section=record.find(([code])=>code===2)[1];
        if (section==='HEADER') {
          let seed=false;
          record=record.filter(([code,value])=>{
            if (code===9) seed=value==='$HANDSEED';
            return !seed;
          });
        }
      } else if (kind==='ENDSEC') section='';
      else if (section==='ENTITIES') {
        entities.push(record);
        continue;
      } else if (section==='TABLES'&&kind==='LAYER') {
        const name=record.find(([code])=>code===2)[1];
        assert.ok(!layers.has(name),'unique DXF layers');
        layers.set(name,record);
        continue;
      } else if (section==='TABLES'&&kind==='TABLE'&&record.some(([code,value])=>code===2&&value==='LAYER')) {
        assert.equal(layerCount,undefined,'one DXF layer table');
        layerCount=Number(record.find(([code])=>code===70)[1]);
        record=record.filter(([code])=>code!==70);
      }
      metadata.push(record);
    }
    assert.equal(layerCount,layers.size,'DXF layer count matches declarations');
    // Dictionary objects after the drawing also shift when a layer is omitted.
    // Keep every field and preserve handle identity plus reference topology.
    const identities=new Map();
    const register=(record,identity)=>{
      record.forEach(([code,value],index)=>{
        if (code===5||code===105) {
          const id=handle(value);
          assert.ok(!identities.has(id),'one identity per DXF object');
          identities.set(id,identity+':'+index);
        }
      });
    };
    entities.forEach((record,index)=>register(record,'entity:'+index));
    for (const [name,record] of layers) register(record,'layer:'+name);
    metadata.forEach((record,index)=>{
      const header=record[0][1]==='SECTION'&&record.some(([code,value])=>code===2&&value==='HEADER');
      if (!header) register(record,'metadata:'+index);
    });
    for (const record of [...entities,...layers.values(),...metadata]) for (const field of record)
      if (field[0]===5||field[0]===105||referenceCode(field[0])) {
        const id=handle(field[1]);
        assert.ok(id==='0'||identities.has(id),'registered DXF object');
        field[1]=id==='0' ? '0' : identities.get(id);
      }
    return {entities,layers,preamble,metadata};
  };
  const observed=parse(actual),reference=parse(expected);
  assert.deepEqual(observed.preamble,reference.preamble,label+' drawing preamble');
  assert.deepEqual(observed.metadata,reference.metadata,label+' complete drawing metadata');
  assert.deepEqual(observed.entities,reference.entities,label+' complete drawing entities');
  const used=new Set(reference.entities.flatMap(record=>record.filter(([code])=>code===8).map(([,name])=>name)));
  for (const layer of used) {
    assert.ok(observed.layers.has(layer),label+' declares used layer '+layer);
    assert.deepEqual(observed.layers.get(layer),reference.layers.get(layer),label+' layer '+layer);
  }
}
