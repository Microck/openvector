/** Legacy TIFF row codecs. Source context and notices: tiff-licenses.txt.
 * Directory validation fixes the sample depth/count before this boundary. */
export function decodeNext(encoded,{width,rows}) {
  const stride=Math.ceil(width/4),output=new Uint8Array(stride*rows).fill(255);
  let offset=0;
  const take=count=>{
    if (offset+count>encoded.length) throw new Error('TIFF NeXT row is incomplete.');
    const start=offset;offset+=count;return start;
  };
  for (let row=0;row<rows;row++) {
    let command=encoded[take(1)];
    if (command===0) {
      const start=take(stride);output.set(encoded.subarray(start,start+stride),row*stride);
    }
    else if (command===64) {
      const header=take(4),start=encoded[header]*256+encoded[header+1],count=encoded[header+2]*256+encoded[header+3];
      if (start+count>stride) throw new Error('TIFF NeXT span exceeds its row.');
      const span=take(count);output.set(encoded.subarray(span,span+count),row*stride+start);
    } else {
      let x=0;
      while (x<width) {
        const gray=command>>>6,count=Math.min(command&63,width-x);
        for (let pixel=0;pixel<count;pixel++,x++) {
          const target=row*stride+(x>>>2),shift=6-(x&3)*2;
          output[target]=(x&3)===0 ? gray<<6 : output[target]|gray<<shift;
        }
        if (x<width) command=encoded[take(1)];
      }
    }
  }
  return output;
}

/** ThunderScan retains one scanline across strips. A terminal run counts
 * pixels but omits its full-byte stores. The TIFF quantum buffer starts at
 * zero; omitted bytes retain that value or the prior row, including at strips. */
export function decodeThunder(encoded,{width,rows,scanline}) {
  const stride=Math.ceil(width/2),output=new Uint8Array(stride*rows),scratch=scanline;
  const deltas2=[0,1,0,-1],deltas3=[0,1,2,3,0,-3,-2,-1];
  let offset=0;
  for (let row=0;row<rows;row++) {
    let x=0,last=0,cursor=0;
    const emit=value=>{
      if (x>=width) throw new Error('TIFF ThunderScan command exceeds its row.');
      last=value&15;
      if (x&1) scratch[cursor++]|=last;
      else scratch[cursor]=last<<4;
      x++;
    };
    while (x<width) {
      if (offset>=encoded.length) throw new Error('TIFF ThunderScan row is incomplete.');
      const command=encoded[offset++],kind=command>>>6;
      if (kind===0) {
        let count=command&63;
        if (x+count>width) throw new Error('TIFF ThunderScan run exceeds its row.');
        if (x&1) {
          scratch[cursor]|=last;last=scratch[cursor++];x++;count--;
        } else last|=last<<4;
        x+=count;
        if (x<width) for (;count>0;count-=2) scratch[cursor++]=last;
        if (count===-1) scratch[--cursor]&=240;
        last&=15;
      } else if (kind===3) emit(command);
      else {
        const bits=kind===1 ? 2 : 3,skip=kind===1 ? 2 : 4,deltas=kind===1 ? deltas2 : deltas3;
        for (let shift=6-bits;shift>=0;shift-=bits) {
          const code=(command>>>shift)&((1<<bits)-1);
          if (code!==skip) emit(last+deltas[code]);
        }
      }
    }
    output.set(scratch,row*stride);
  }
  return output;
}
