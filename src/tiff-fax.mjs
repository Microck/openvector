// CCITT run-code tables adapted from UTIF.js commit588351078202eebccdb3ba996f5b98c640e4265a.
// Copyright(c)2017 Photopea. MIT license, retained in tiff-licenses.txt.
// The bitstream decoder below is independently implemented.

const runLengths = ( function()
{
	var addKeys = function(lens, arr, i0, inc) {  for(var i=0; i<arr.length; i++) lens[arr[i]] = i0 + i*inc;  }

	var termW = "00110101,000111,0111,1000,1011,1100,1110,1111,10011,10100,00111,01000,001000,000011,110100,110101," // 15
	+ "101010,101011,0100111,0001100,0001000,0010111,0000011,0000100,0101000,0101011,0010011,0100100,0011000,00000010,00000011,00011010," // 31
	+ "00011011,00010010,00010011,00010100,00010101,00010110,00010111,00101000,00101001,00101010,00101011,00101100,00101101,00000100,00000101,00001010," // 47
	+ "00001011,01010010,01010011,01010100,01010101,00100100,00100101,01011000,01011001,01011010,01011011,01001010,01001011,00110010,00110011,00110100";

	var termB = "0000110111,010,11,10,011,0011,0010,00011,000101,000100,0000100,0000101,0000111,00000100,00000111,000011000," // 15
	+ "0000010111,0000011000,0000001000,00001100111,00001101000,00001101100,00000110111,00000101000,00000010111,00000011000,000011001010,000011001011,000011001100,000011001101,000001101000,000001101001," // 31
	+ "000001101010,000001101011,000011010010,000011010011,000011010100,000011010101,000011010110,000011010111,000001101100,000001101101,000011011010,000011011011,000001010100,000001010101,000001010110,000001010111," // 47
	+ "000001100100,000001100101,000001010010,000001010011,000000100100,000000110111,000000111000,000000100111,000000101000,000001011000,000001011001,000000101011,000000101100,000001011010,000001100110,000001100111";

	var makeW = "11011,10010,010111,0110111,00110110,00110111,01100100,01100101,01101000,01100111,011001100,011001101,011010010,011010011,011010100,011010101,011010110,"
	+ "011010111,011011000,011011001,011011010,011011011,010011000,010011001,010011010,011000,010011011";

	var makeB = "0000001111,000011001000,000011001001,000001011011,000000110011,000000110100,000000110101,0000001101100,0000001101101,0000001001010,0000001001011,0000001001100,"
	+ "0000001001101,0000001110010,0000001110011,0000001110100,0000001110101,0000001110110,0000001110111,0000001010010,0000001010011,0000001010100,0000001010101,0000001011010,"
	+ "0000001011011,0000001100100,0000001100101";

	var makeA = "00000001000,00000001100,00000001101,000000010010,000000010011,000000010100,000000010101,000000010110,000000010111,000000011100,000000011101,000000011110,000000011111";

	termW = termW.split(",");  termB = termB.split(",");  makeW = makeW.split(",");  makeB = makeB.split(",");  makeA = makeA.split(",");

	var lensW = {}, lensB = {};
	addKeys(lensW, termW, 0, 1);  addKeys(lensW, makeW, 64,64);  addKeys(lensW, makeA, 1792,64);
	addKeys(lensB, termB, 0, 1);  addKeys(lensB, makeB, 64,64);  addKeys(lensB, makeA, 1792,64);
	return [lensW, lensB];
} )();

const verticalCodes={'1':0,'011':1,'000011':2,'0000011':3,'010':-1,'000010':-2,'0000010':-3};

/** Decode CCITT RLE/T.4/T.6 to canonical MSB-first packed scanlines. The file's
 * fill order applies to the compressed bitstream, not the returned bytes. */
export function decodeFax(encoded,{width,rows,compression,fillOrder,t4Options}) {
  const rowBytes=Math.ceil(width/8),output=new Uint8Array(rowBytes*rows);
  let bit=0;
  const readBit=()=>{
    if (bit>=encoded.length*8) throw new Error('TIFF CCITT bitstream is incomplete.');
    const shift=fillOrder===2 ? bit&7 : 7-(bit&7);
    const byte=encoded[bit>>>3];bit++;
    return (byte>>>shift)&1;
  };
  const readRun=color=>{
    let length=0;
    while (true) {
      let code='',part;
      for (let size=1;size<=13;size++) {
        code+=readBit();part=runLengths[color][code];
        if (part!==undefined) break;
      }
      if (part===undefined) throw new Error('TIFF CCITT run code is invalid.');
      length+=part;
      if (length>width) throw new Error('TIFF CCITT run exceeds the scanline.');
      if (part<64) return length;
    }
  };
  const endOfLine=()=>{
    let zeros=0;
    while (readBit()===0) zeros++;
    if (zeros<11) throw new Error('TIFF Group3 end-of-line code is missing.');
  };
  const differences=line=>{
    const changes=[];
    if (line[0]===1) changes.push([0,1]);
    for (let x=1;x<width;x++) if (line[x]!==line[x-1]) changes.push([x,line[x]]);
    changes.push([width,0],[width,1]);return changes;
  };
  let previous=new Uint8Array(width);
  for (let row=0;row<rows;row++) {
    let oneDimensional=compression===2||compression===32771;
    if (compression===3) {
      endOfLine();oneDimensional=(t4Options&1)===0||readBit()===1;
    }
    const line=new Uint8Array(width);
    let position=0,color=0;
    const paint=end=>{
      if (end<position||end>width) throw new Error('TIFF CCITT transition is outside the scanline.');
      if (color) line.fill(1,position,end);
      position=end;
    };
    if (oneDimensional) {
      while (position<width) {paint(position+readRun(color));color^=1;}
    } else {
      const changes=differences(previous);
      let reference=0;
      while (position<width) {
        // Anchor the cursor to a0, not the previous b1. A left vertical mode
        // can still need a reference change earlier than that previous b1.
        const at=position+(position===0 ? 0 : 1);
        while (changes[reference][0]<at) reference++;
        let first=reference;
        while (changes[first][1]!==1-color) first++;
        const b1=changes[first][0];
        let second=reference;
        while (changes[second][0]<b1||changes[second][1]!==color) second++;
        const b2=changes[second][0];
        let code='',mode;
        for (let size=1;size<=7;size++) {
          code+=readBit();
          if (code==='0001') {mode='pass';break;}
          if (code==='001') {mode='horizontal';break;}
          if (verticalCodes[code]!==undefined) {mode='vertical';break;}
        }
        if (mode==='pass') paint(b2);
        else if (mode==='horizontal') {
          paint(position+readRun(color));color^=1;
          paint(position+readRun(color));color^=1;
        } else if (mode==='vertical') {paint(b1+verticalCodes[code]);color^=1;}
        else throw new Error('TIFF CCITT two-dimensional code is unsupported or invalid.');
      }
    }
    for (let x=0;x<width;x++) if (line[x]) output[row*rowBytes+(x>>>3)]|=1<<(7-(x&7));
    previous=line;
    if (compression===2||compression===32771) {
      const alignment=compression===32771 ? 16 : 8;
      bit=Math.ceil(bit/alignment)*alignment;
    }
  }
  return output;
}
