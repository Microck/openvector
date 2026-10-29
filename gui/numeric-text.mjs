// This is Qt 4.4.3's complete UTF-16 space inventory, measured through QChar.
// Modern Unicode whitespace differs at U+180E and U+FEFF.
const spaces=new Set([9,10,11,12,13,32,160,5760,6158,8192,8193,8194,8195,8196,8197,8198,
  8199,8200,8201,8202,8232,8233,8239,8287,12288]);
export const isNumericSpace=code=>spaces.has(code);
const cLocale=Object.freeze({decimal:'.',group:',',zero:'0',positive:'+',negative:'-',exponent:'e'});
const failed=()=>({value:0,ok:false});

/** Normalize a complete Qt numeric token. Mapping precedes trailing-space
 * handling: a locale's space-like group separator still counts as a group. */
function normalize(text,locale) {
  let index=0,normalized='';
  while(index<text.length && isNumericSpace(text.charCodeAt(index)))index++;
  if(index===text.length)return null;
  const zero=locale.zero.charCodeAt(0);
  for(;index<text.length;index++) {
    const character=text[index],code=text.charCodeAt(index);
    if(code>=zero && code<zero+10)normalized+=String(code-zero);
    else if(code>=48 && code<=57)normalized+=character;
    else if(character===locale.positive)normalized+='+';
    else if(character===locale.negative)normalized+='-';
    else if(character===locale.decimal)normalized+='.';
    else if(character===locale.group || (locale.group==='\u00a0' && character===' '))normalized+=',';
    else if(character===locale.exponent || character===locale.exponent.toUpperCase())normalized+='e';
    else if((code>=65 && code<=90) || (code>=97 && code<=122))normalized+=character.toLowerCase();
    else {
      for(;index<text.length;index++)if(!isNumericSpace(text.charCodeAt(index)))return null;
      break;
    }
  }
  return normalized;
}

function integerToken(token) {
  if(token===null || !/^[+-]?\d+$/.test(token))return failed();
  const value=BigInt(token);
  return value>=-2147483648n && value<=2147483647n ? {value:Number(value),ok:true}:failed();
}

function doubleToken(token) {
  if(token==='nan')return {value:NaN,ok:true};
  if(token==='inf' || token==='+inf')return {value:Infinity,ok:true};
  if(token==='-inf')return {value:-Infinity,ok:true};
  if(token===null || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/.test(token))return failed();
  const value=Number(token);
  // Qt rejects overflow and a nonzero significand that underflows to zero.
  // A syntactic zero, including negative zero, remains a successful result.
  if(!Number.isFinite(value) || (value===0 && /[1-9]/.test(token.split('e')[0])))return failed();
  return {value,ok:true};
}

function convert(text,locales,parser) {
  for(const locale of locales) {
    const parsed=parser(normalize(text,locale));
    if(parsed.ok)return parsed;
  }
  return failed();
}

function requireText(text) {
  if(typeof text!=='string')throw new Error('Numeric text must be a string.');
}

/** Session locale is supplied by the host. Own it once; each operation returns
 * a new result, including failures, so callers cannot mutate parser state. */
export function createNumericText(locale) {
  if(!locale || typeof locale!=='object' || Array.isArray(locale))throw new Error('Provide numeric locale symbols.');
  const owned={};
  for(const name of Object.keys(cLocale)) {
    if(typeof locale[name]!=='string' || locale[name].length!==1)
      throw new Error('Numeric locale '+name+' must be one UTF-16 code unit.');
    owned[name]=locale[name];
  }
  const locales=[Object.freeze(owned),cLocale];
  return Object.freeze({
    integer(text) {requireText(text);return convert(text,locales,integerToken);},
    double(text) {requireText(text);return convert(text,locales,doubleToken);},
    percent(text) {
      requireText(text);
      const parsed=convert(text.endsWith('%')?text.slice(0,-1):text,locales,doubleToken);
      return {value:parsed.value*0.01,ok:parsed.ok};
    },
  });
}

/** Fixed decimal formatting rounds the exact binary64 value to an even tie.
 * BigInt keeps binary ties and large integers exact without decimal coercion. */
export function formatFixedOne(value) {
  if(typeof value!=='number')throw new Error('Format a numeric value.');
  if(Number.isNaN(value))return 'nan';
  if(!Number.isFinite(value))return value<0?'-inf':'inf';
  const view=new DataView(new ArrayBuffer(8));view.setFloat64(0,Math.abs(value));
  const bits=view.getBigUint64(0),biased=Number(bits>>52n);
  const mantissa=(bits&0xfffffffffffffn)|(biased===0?0n:0x10000000000000n);
  const exponent=biased===0?-1074:biased-1075;
  let numerator=mantissa*10n,denominator=1n;
  if(exponent>=0)numerator<<=BigInt(exponent);
  else denominator<<=BigInt(-exponent);
  let rounded=numerator/denominator;
  const doubledRemainder=(numerator%denominator)*2n;
  if(doubledRemainder>denominator || (doubledRemainder===denominator && (rounded&1n)!==0n))rounded++;
  return (value<0?'-':'')+(rounded/10n)+'.'+(rounded%10n);
}
