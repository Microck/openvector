import {zoomGetterScales,zoomSliderScales,zoomInverseThresholds} from './viewer-zoom-values.mjs';
import {scaledViewerGeometry,fittedViewerGeometry,centeredViewerGeometry,wheelViewerGeometry} from './viewer-geometry.mjs';

const clampPosition=value=>Math.max(0,Math.min(300,value));

/** The inverse is a monotone integer-valued function. Its generated transition
 * boundaries preserve the native truncation, including values beside a step. */
export function viewerScalePosition(scale) {
  if(!Number.isFinite(scale))throw new Error('Viewer scale must be finite.');
  scale=Math.max(0.2,Math.min(24,scale));
  let lower=0,upper=zoomInverseThresholds.length;
  while(lower<upper) {
    const middle=(lower+upper)>>>1;
    if(scale>=zoomInverseThresholds[middle])lower=middle+1;
    else upper=middle;
  }
  return lower;
}

/** From View reads the logarithmic slider getter, not the displayed matrix. */
export const viewerExportScale=view=>zoomGetterScales[view.slider];

export function viewerDirectScale(view,scale) {
  const slider=viewerScalePosition(scale),zoom=Math.max(0.2,Math.min(24,scale));
  return scaledViewerGeometry(view,zoom,slider);
}

export function viewerFit(view,image,area) {
  const scale=Math.min(area.width/image.width,area.height/image.height);
  const zoom=Math.max(0.2,Math.min(24,scale));
  return fittedViewerGeometry(view,image,area,zoom,viewerScalePosition(scale));
}

export const viewerOne=view=>centeredViewerGeometry(viewerDirectScale(view,1));

/** A slider change preserves the retained logical center while quantizing
 * its displayed position through the integer scrollbars. */
export function viewerSlider(view,position) {
  if(!Number.isInteger(position))throw new Error('Viewer slider position must be an integer.');
  const slider=clampPosition(position),zoom=Math.max(0.2,Math.min(24,zoomSliderScales[slider]));
  return scaledViewerGeometry(view,zoom,slider);
}

export function viewerWheel(view,delta,point) {
  if(!Number.isInteger(delta) || delta< -2147483648 || delta>2147483647)
    throw new Error('Viewer wheel angle must be a signed integer.');
  const next=viewerSlider(view,view.slider+Math.trunc(delta/12));
  const shift=1/viewerExportScale(view)-1/viewerExportScale(next);
  return wheelViewerGeometry(next,{x:view.center.x+point.x*shift,y:view.center.y+point.y*shift});
}

export function browserViewerWheel(event) {
  if(!Number.isInteger(event.wheelDeltaY))
    throw new Error('This browser does not expose the native wheel angle. Use the zoom slider.');
  return event.wheelDeltaY;
}
