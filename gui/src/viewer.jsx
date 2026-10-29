import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Focus, Image, Layers, MousePointer2, Scan, ZoomIn, ZoomOut } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Slider } from '@/components/ui/slider';
import { browserViewerPointer } from '../viewer-input.mjs';
import {browserViewerWheel,viewerOne,viewerFit,viewerSlider,viewerWheel} from '../viewer-zoom.mjs';
import {viewerImagePoint,viewerPan,viewerScroll,viewerResize} from '../viewer-geometry.mjs';

export function IconButton({ label, children, ...props }) {
  return <Button type="button" variant="ghost" size="icon" aria-label={label} title={label} {...props}>{children}</Button>;
}

export function Bitmap({ bitmap, label='Source bitmap' }) {
  const canvas = useRef(null);
  useEffect(() => {
    const bytes = Uint8ClampedArray.from(atob(bitmap.rgba), character => character.charCodeAt(0));
    canvas.current.getContext('2d').putImageData(new ImageData(bytes, bitmap.width, bitmap.height), 0, 0);
  }, [bitmap]);
  return <canvas ref={canvas} width={bitmap.width} height={bitmap.height} aria-label={label} aria-hidden={label===null?true:undefined} />;
}

const svgSource = (svg, paths) => 'data:image/svg+xml;base64,' + btoa(paths
  ? svg.replaceAll('<path ', '<path stroke="#ffb86b" stroke-width="0.6" vector-effect="non-scaling-stroke" ')
  : svg);

function ViewScrollbar({axis,bar,onChange}) {
  const vertical=axis==='vertical',label=vertical?'Vertical image position':'Horizontal image position';
  const className=`view-scrollbar ${axis}`;
  // Radix's percentage calculation requires a nonempty range. A fitted axis
  // has no scroll position to edit, so expose its actual zero range as disabled.
  if(bar.minimum===bar.maximum)return <div className={className} role="scrollbar" aria-label={label}
    aria-controls="image-viewport" aria-orientation={axis} aria-valuemin={bar.minimum} aria-valuemax={bar.maximum}
    aria-valuenow={bar.value} aria-disabled="true" />;
  return <Slider className={className} orientation={axis} inverted={vertical}
    min={bar.minimum} max={bar.maximum} step={1} value={[bar.value]} aria-label={label}
    style={{'--scroll-thumb-size':`${Math.max(8,bar.pageStep*bar.pageStep/(bar.maximum-bar.minimum+bar.pageStep))}px`}}
    onValueChange={([value])=>onChange(value)} onKeyDown={event=>{
      const direction={ArrowLeft:-1,ArrowUp:-1,ArrowRight:1,ArrowDown:1,PageUp:-1,PageDown:1}[event.key];
      if(direction) {
        event.preventDefault();onChange(bar.value+direction*(event.key.startsWith('Page')?bar.pageStep:bar.singleStep));
      } else if(event.key==='Home' || event.key==='End') {
        event.preventDefault();onChange(event.key==='Home'?bar.minimum:bar.maximum);
      }
    }} />;
}

/** One layered viewer, matching the reference layout. Temporary B/V holds
 * never overwrite the default layer chosen by the buttons. */
export function Viewer({ document, onOpen, disabled, layer, shownLayer, onLayerChange,editing,backgroundEditing,editTool,onGesture,viewModel }) {
  const viewport = useRef(null);
  const drag = useRef(null);
  const [view, renderView] = useState(()=>viewModel.current);
  const setView=update=>{
    // The export controls read this same view. Keep the model current at the
    // input event, without forcing the whole wizard to render for every pan.
    viewModel.current=typeof update==='function'?update(viewModel.current):update;
    renderView(viewModel.current);
  };
  const [paths, setPaths] = useState(false);
  const availableVector = Boolean(document?.preview);
  const vectorSource = useMemo(() => document?.preview ? svgSource(document.preview.svg, paths) : null, [document?.preview?.svg, paths]);
  const defaultLayer = (layer === 'vector' && !availableVector || layer === 'segments' && !document?.segments) ? 'bitmap' : layer;
  const shown = (shownLayer === 'vector' && !availableVector || shownLayer === 'segments' && !document?.segments) ? 'bitmap' : shownLayer;
  const viewportSize=()=>({width:viewport.current.clientWidth,height:viewport.current.clientHeight});
  const fit = () => document && setView(previous=>viewerFit(previous,document,viewportSize()));

  useEffect(() => {
    const observer = new ResizeObserver(() => setView(previous=>viewerResize(previous,viewportSize())));
    observer.observe(viewport.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (document?.bitmap) {
      setView(previous=>viewerFit(previous,document,viewportSize()));
    }
  }, [document?.bitmap]);

  const changeSlider = step => setView(previous=>viewerSlider(previous,previous.slider+step));
  const step = (axis,direction,page=false) => setView(previous=>{
    const bar=previous.bars[axis==='horizontal'?0:1];
    return viewerScroll(previous,axis,bar.value+direction*(page?bar.pageStep:bar.singleStep));
  });
  const stopDrag = () => { drag.current = null; };
  const viewportPoint = event => {
    const bounds=viewport.current.getBoundingClientRect();
    return {x:Math.trunc(event.clientX-bounds.left),y:Math.trunc(event.clientY-bounds.top)};
  };
  const imagePoint = event => {
    const point=viewerImagePoint(viewModel.current,viewportPoint(event));
    return [point.x,point.y];
  };
  const handlePointer = event => {
    const gesture=browserViewerPointer(event);
    if(gesture.phase==='release') {
      const picking=!disabled && editing && editTool==='picker';
      if(drag.current?.painting || picking)onGesture({...gesture,point:imagePoint(event)});
      stopDrag();return;
    }
    if(!document)return;
    if(gesture.phase==='press') {
      if(editing && editTool!=='pan') {
        // Layer inspection changes rendering, not the active editing tool.
        if(disabled || !(gesture.buttons&1))return;
        drag.current={painting:true};onGesture({...gesture,point:imagePoint(event)});
      } else {
        if(gesture.button!==1)return;
        drag.current={point:viewportPoint(event),view:viewModel.current};
      }
      event.currentTarget.setPointerCapture(event.pointerId);return;
    }
    // A held-button move may enter without a press. Keep later samples flowing
    // while requests are pending, but never turn a button change into motion.
    if(!drag.current && !disabled && editing && !backgroundEditing && editTool==='pencil' && (gesture.buttons&1)) {
      drag.current={painting:true};event.currentTarget.setPointerCapture(event.pointerId);
    }
    if(drag.current?.painting)onGesture({...gesture,point:imagePoint(event)});
    if(drag.current?.view) {
      const point=viewportPoint(event);
      setView(viewerPan(drag.current.view,{x:point.x-drag.current.point.x,y:point.y-drag.current.point.y}));
    }
  };
  return <section className="viewer" aria-label="Image workspace">
    <aside className="viewer-tools" aria-label="Viewer controls">
      <div className="tool-group">
        <IconButton label="Pan up" disabled={!document} onClick={() => step('vertical',1)}><ArrowUp /></IconButton>
        <div className="tool-pair">
          <IconButton label="Pan left" disabled={!document} onClick={() => step('horizontal',-1)}><ArrowLeft /></IconButton>
          <IconButton label="Pan right" disabled={!document} onClick={() => step('horizontal',1)}><ArrowRight /></IconButton>
        </div>
        <IconButton label="Pan down" disabled={!document} onClick={() => step('vertical',-1)}><ArrowDown /></IconButton>
      </div>
      <div className="tool-group zoom-tools">
        <IconButton label="Zoom in" disabled={!document} onClick={() => changeSlider(1)}><ZoomIn /></IconButton>
        <Slider className="zoom-slider" orientation="vertical" min={0} max={300} step={1}
          value={[view.slider]} disabled={!document} aria-label="Zoom"
          onValueChange={([value]) => setView(previous=>viewerSlider(previous,value))} />
        <IconButton label="Zoom out" disabled={!document} onClick={() => changeSlider(-1)}><ZoomOut /></IconButton>
        <Button variant="ghost" size="sm" className="percent-button" disabled={!document}
          aria-label="100 percent zoom" onClick={() => setView(viewerOne)}>100%</Button>
        <IconButton label="Fit image" disabled={!document} onClick={fit}><Focus /></IconButton>
      </div>
      <div className="tool-group layer-tools">
        <IconButton label="Bitmap layer (hold B)" disabled={!document} aria-pressed={defaultLayer === 'bitmap'}
          onClick={() => onLayerChange('bitmap')}><Image /></IconButton>
        <IconButton label="Segmentation layer" disabled={!document?.segments} aria-pressed={defaultLayer === 'segments'}
          onClick={() => onLayerChange('segments')}><Layers /></IconButton>
        <IconButton label="Vector layer (hold V)" disabled={!availableVector} aria-pressed={defaultLayer === 'vector'}
          onClick={() => onLayerChange('vector')}><Scan /></IconButton>
        <label className="paths-control"><Checkbox checked={paths} disabled={!availableVector} onCheckedChange={setPaths} />Paths</label>
      </div>
    </aside>
    <div className="viewer-stage">
    <div id="image-viewport" className={`viewport ${document ? 'has-image' : ''} ${editing && editTool !== 'pan' ? 'painting' : ''}`} ref={viewport} tabIndex={0} aria-label="Image viewer"
      onWheel={event => { if (document) {
        const pixel=viewportPoint(event),point={x:pixel.x-viewModel.current.area.width/2,y:pixel.y-viewModel.current.area.height/2};
        setView(previous=>viewerWheel(previous,browserViewerWheel(event.nativeEvent),point));
      } }}
      onPointerDown={handlePointer} onPointerMove={handlePointer} onPointerUp={handlePointer}
      onPointerCancel={stopDrag} onLostPointerCapture={stopDrag}
      onKeyDown={event => {
        const movement={ArrowLeft:['horizontal',-1],ArrowRight:['horizontal',1],ArrowUp:['vertical',-1],
          ArrowDown:['vertical',1],PageUp:['vertical',-1,true],PageDown:['vertical',1,true]}[event.key];
        if(document && movement && !event.altKey) {event.preventDefault();step(...movement);}
      }}>
      {document ? <div className="image-layer" style={{ width: document.width, height: document.height,
        backgroundSize:`${16/view.zoom}px ${16/view.zoom}px`,
        transform: `translate(${view.origin.x}px, ${view.origin.y}px) scale(${view.zoom})` }}>
        {shown === 'vector' && availableVector
          ? <><img className="vector-image" src={vectorSource} alt="Vector result" draggable="false" />
            {document.preview.outlines.length > 0 && <svg className="removed-outlines" width={document.width} height={document.height} viewBox={`0 0 ${document.width} ${document.height}`} aria-label="Removed region boundaries">
              {document.preview.outlines.map((path,index) => <path key={index} d={path} fill="none" stroke="#f44" strokeWidth={1/view.zoom} strokeDasharray={`${4/view.zoom} ${4/view.zoom}`} />)}
            </svg>}</>
          : <Bitmap bitmap={shown === 'segments' ? document.segments : document.bitmap} />}
      </div> : <div className="empty-view">
        <svg className="empty-mark" viewBox="0 0 160 112" aria-hidden="true"><path d="M16 88 53 20h21L37 88ZM77 88l29-54 29 54Z" fill="none" stroke="currentColor" strokeWidth="1.5"/>
          <path d="M53 20h21L37 88H16Z" stroke="currentColor" fill="none" strokeDasharray="2 5"/>
          <rect x="49" y="16" width="8" height="8" fill="#000" stroke="currentColor"/><rect x="12" y="84" width="8" height="8" fill="#000" stroke="currentColor"/>
          <rect x="131" y="84" width="8" height="8" fill="#000" stroke="currentColor"/></svg>
        <h1>From pixels to paths.</h1>
        <p>Drop an image here to get started.</p>
        <Button variant="outline" onClick={onOpen} disabled={disabled}><Image />Open image</Button>
        <span className="file-hint">PNG, BMP, JPEG, GIF or TIFF <span>·</span> Ctrl O</span>
      </div>}
      {document && <div className="viewer-readout" aria-live="polite"><MousePointer2 size={12} />{shown === 'vector' ? 'Vector' : shown === 'segments' ? 'Segments' : 'Bitmap'}<span>{Math.round(view.zoom * 100)}%</span></div>}
    </div>
    <ViewScrollbar axis="horizontal" bar={view.bars[0]} onChange={value=>setView(previous=>viewerScroll(previous,'horizontal',value))} />
    <ViewScrollbar axis="vertical" bar={view.bars[1]} onChange={value=>setView(previous=>viewerScroll(previous,'vertical',value))} />
    <div className="view-scroll-corner" aria-hidden="true" />
    </div>
  </section>;
}
