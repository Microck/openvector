import {renderBitmap} from './bitmap-raster.mjs';
import {encodeBmp,encodeJpeg,encodePng,encodeTiff} from './bitmap-codecs.mjs';

const encoders={bmp:encodeBmp,jpg:encodeJpeg,jpeg:encodeJpeg,png:encodePng,tif:encodeTiff,tiff:encodeTiff};
export const bitmapFormats=Object.freeze(Object.keys(encoders));

/** Export the document's completed fitted scene using native storage policy.
 * The document owns source-alpha classification and background edit history;
 * a format's alpha capability or hidden-region list cannot replace either. */
export function exportBitmap(document,{format,scale=1,flatten=false}) {
  if (!Object.hasOwn(encoders,format)) throw new Error('Choose a bitmap format: BMP, JPEG, PNG or TIFF.');
  if (typeof flatten!=='boolean') throw new Error('Bitmap flatten must be a boolean.');
  if (flatten && format==='png') throw new Error('Flatten confirmation applies only to non-PNG bitmap output.');
  const state=document.snapshot();
  if (!state.fitting?.complete) throw new Error('Finish vectorizing the image before exporting.');
  const alphaType=document.sourceAlphaType(),background=document.backgroundState();
  if (format!=='png' && alphaType!==0 && state.settings.backgroundMode===0 && !flatten) {
    const error=new Error('Non-PNG bitmap export may discard transparency. Confirm flattening before exporting.');
    error.code='BITMAP_FLATTEN_REQUIRED';
    throw error;
  }
  const hasAlpha=background.undo.length!==0 ||
    (format==='png' && alphaType!==0 && ![1,2].includes(state.settings.backgroundMode));
  const rendered=renderBitmap({geometry:state.fitting.geometry,width:state.original.width,height:state.original.height,
    scale,hasAlpha,hiddenRegions:background.hiddenRegions});
  if (hasAlpha && ['png','tif','tiff'].includes(format)) {
    // Qt's PNG/TIFF paths convert to straight channels. Its BMP/JPEG writers
    // read stored RGB directly, including premultiplied background edits.
    for (let offset=0;offset<rendered.bgra.length;offset+=4) {
      const alpha=rendered.bgra[offset+3];
      for (let channel=0;channel<3;channel++)
        rendered.bgra[offset+channel]=alpha?Math.floor(rendered.bgra[offset+channel]*255/alpha):0;
    }
  }
  return encoders[format](rendered);
}
