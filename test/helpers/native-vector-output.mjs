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
