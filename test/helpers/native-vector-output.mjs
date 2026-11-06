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
