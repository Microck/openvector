import {zoomGetterScales} from './viewer-zoom-values.mjs';

const bar=()=>({value:0,minimum:0,maximum:0,singleStep:1,pageStep:10});
const clamp=(value,minimum,maximum)=>Math.max(minimum,Math.min(maximum,value));
const roundBound=value=>value<=-2147483648?-2147483648:value>=2147483647?2147483647:Math.floor(value+0.5);

export function initialViewerView() {
  return {zoom:1,slider:100,image:{width:0,height:0},area:{width:0,height:0},
    center:{x:0,y:0},qtCenter:{x:0,y:0},bars:[bar(),bar()],indent:{x:0,y:0},
    scene:{x:0,y:0,width:0,height:0},origin:{x:0,y:0},retainCenterOnResize:true};
}

function draft(view) {
  return {...view,image:{...view.image},area:{...view.area},center:{...view.center},qtCenter:{...view.qtCenter},
    bars:view.bars.map(value=>({...value})),indent:{...view.indent},scene:{...view.scene},origin:{...view.origin}};
}

function presented(view) {
  view.origin={x:-Math.trunc(-view.indent.x)-view.bars[0].value,y:-Math.trunc(-view.indent.y)-view.bars[1].value};
  return view;
}

/** Qt 4.4.3 inverts the transform through adjoint/determinant. Replacing
 * this with direct division changes observed point coordinates by one bit. */
export function viewerImagePoint(view,point) {
  const inverse=view.zoom*(1/(view.zoom*view.zoom));
  return {x:(point.x+view.bars[0].value+Math.trunc(-view.indent.x))*inverse,
    y:(point.y+view.bars[1].value+Math.trunc(-view.indent.y))*inverse};
}

function scrolled(view,x,y,notify) {
  const next=[x,y].map((value,index)=>clamp(Math.trunc(value),view.bars[index].minimum,view.bars[index].maximum));
  if(next.some((value,index)=>value!==view.bars[index].value)) {
    for(let index=0;index<2;index++)view.bars[index].value=next[index];
    view.qtCenter=viewerImagePoint(view,{x:Math.trunc((view.area.width-1)/2)+0,y:Math.trunc((view.area.height-1)/2)+0});
    if(notify)view.center=viewerImagePoint(view,{x:Math.trunc(view.area.width/2+0.5),y:Math.trunc(view.area.height/2+0.5)});
  }
}

function sceneBounds(view,notify) {
  if(view.image.width===0 || view.image.height===0) {
    view.scene={x:0,y:0,width:0,height:0};view.indent={x:0,y:0};
    for(const scroll of view.bars)scroll.value=scroll.minimum=scroll.maximum=0;
    return;
  }
  const inverse=1/zoomGetterScales[view.slider];
  const halfWidth=(view.area.width*0.5)*inverse,halfHeight=(view.area.height*0.5)*inverse;
  view.scene={x:-halfWidth,y:-halfHeight,width:halfWidth*2+view.image.width,height:halfHeight*2+view.image.height};
  const saved={...view.qtCenter};
  for(const [index,axis,dimension] of [[0,'x','width'],[1,'y','height']]) {
    const left=view.scene[axis]*view.zoom,width=view.scene[dimension]*view.zoom;
    const minimum=roundBound(left),maximum=roundBound(left+width-view.area[dimension]);
    const scroll=view.bars[index];
    if(minimum>=maximum) {
      scroll.minimum=scroll.maximum=0;
      view.indent[axis]=Math.trunc(view.area[dimension]/2)-(left+(left+width))/2;
    } else {
      scroll.minimum=minimum;scroll.maximum=maximum;scroll.pageStep=view.area[dimension];
      scroll.singleStep=Math.trunc(view.area[dimension]/20);view.indent[axis]=0;
    }
  }
  scrolled(view,view.bars[0].value,view.bars[1].value,notify);
  view.qtCenter=saved;
}

function centerOn(view,point,notify) {
  const center={x:clamp(point.x,0,view.image.width),y:clamp(point.y,0,view.image.height)};
  view.center={...center};
  scrolled(view,view.indent.x?view.bars[0].value:Math.trunc(center.x*view.zoom-view.area.width/2),
    view.indent.y?view.bars[1].value:Math.trunc(center.y*view.zoom-view.area.height/2),notify);
  view.qtCenter=center;
}

export function scaledViewerGeometry(view,zoom,slider) {
  const next=draft(view);next.zoom=zoom;next.slider=slider;
  // Scaling suppresses native updateCenter callbacks. The requested center
  // remains distinct from integer scrollbar placement until a later action.
  sceneBounds(next,false);centerOn(next,next.center,false);
  return presented(next);
}

export function fittedViewerGeometry(view,image,area,zoom,slider) {
  const source=draft(view);source.image={width:image.width,height:image.height};source.area={width:area.width,height:area.height};
  const next=scaledViewerGeometry(source,zoom,slider);
  centerOn(next,{x:image.width/2,y:image.height/2},true);
  return presented(next);
}

export function centeredViewerGeometry(view) {
  const next=draft(view);centerOn(next,{x:next.image.width/2,y:next.image.height/2},true);return presented(next);
}

export function wheelViewerGeometry(view,center) {
  const next=draft(view);centerOn(next,center,false);return presented(next);
}

export function viewerScroll(view,axis,value) {
  if(!['horizontal','vertical'].includes(axis) || !Number.isInteger(value))throw new Error('Choose a scrollbar and an integer position.');
  const current=view.bars[axis==='horizontal'?0:1];
  if(clamp(value,current.minimum,current.maximum)===current.value)return view;
  const next=draft(view),positions=next.bars.map(value=>value.value);positions[axis==='horizontal'?0:1]=value;
  scrolled(next,...positions,true);return presented(next);
}

export function viewerPan(view,delta) {
  const next=draft(view);scrolled(next,next.bars[0].value-Math.trunc(delta.x),next.bars[1].value-Math.trunc(delta.y),true);
  return presented(next);
}

export function viewerResize(view,area) {
  const next=draft(view);next.area={width:area.width,height:area.height};
  sceneBounds(next,true);
  if(!next.retainCenterOnResize)next.qtCenter=viewerImagePoint(next,{x:Math.trunc((area.width-1)/2)+0,y:Math.trunc((area.height-1)/2)+0});
  next.retainCenterOnResize=false;
  return presented(next);
}
