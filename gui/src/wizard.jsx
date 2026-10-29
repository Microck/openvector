import { ArrowLeft, ArrowRight, Check, ChevronRight, ChevronDown, ChevronUp, ChevronsDown, ChevronsUp, Download, Eraser, ExternalLink, FastForward, FolderOpen, Hand, Layers, PaintBucket, Palette, Pencil, Pipette, Play, Plus, Redo2, RotateCcw, Save, Scan, SlidersHorizontal, Sparkles, Trash2, Undo2, Upload, Zap } from 'lucide-react';
import {useRef,useState} from 'react';
import {quantizeQColorChannels} from '../../src/pixel-input.mjs';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Slider } from '@/components/ui/slider';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {Bitmap} from './viewer';
import {bitmapScaleButtons,bitmapScaleMore} from '../bitmap-size.mjs';

export const pageTitles = { load: 'Load image', mode: 'Wizard mode', type: 'Image type', transparency: 'Transparency',
  quality: 'Input quality', colors: 'Color mode', palette: 'Palette selection', segmentation: 'Image segmentation',
  smoothing: 'Path smoothing', review: 'Review result', editing: 'Edit segmentation', background: 'Remove background', export: 'Export result' };

export function MenuSelect({ label, value, options, onChange, disabled }) {
  return <div className="field"><label>{label}</label><Select value={String(value)} onValueChange={onChange} disabled={disabled}>
    <SelectTrigger aria-label={label}><SelectValue /></SelectTrigger>
    <SelectContent>{options.map(([value, title]) => <SelectItem key={value} value={String(value)}>{title}</SelectItem>)}</SelectContent>
  </Select></div>;
}

function Choice({ title, description, icon: Icon, selected, disabled, onClick }) {
  return <Button variant="ghost" className="choice" disabled={disabled} aria-pressed={selected} onClick={onClick}>
    {Icon && <Icon className="choice-icon" />}<span><strong>{title}</strong>{description && <small>{description}</small>}</span>
    {selected ? <Check className="choice-end" /> : <ChevronRight className="choice-end" />}
  </Button>;
}

function SaveLocation({id,choice,busy,onChange,onBrowse,onSave,savedFile}) {
  const lastCommit=useRef(Promise.resolve());
  const commit=(field,event)=>{
    const input=event.currentTarget;
    const pending=input.value===choice[field]?Promise.resolve(choice):onChange({[field]:input.value});
    lastCommit.current=pending;
    pending.then(next=>{if(input.isConnected)input.value=next[field];},()=>{if(input.isConnected)input.value=choice[field];});
  };
  const save=event=>{
    const opener=event.currentTarget;
    // A Save click follows blur. Do not save the old target if that field's
    // commit failed; the parent already reports its validation error.
    lastCommit.current.then(()=>onSave(opener),()=>{});
  };
  return <>
    <div className="field"><label htmlFor={id+'-directory'}>Directory</label><div className="profile-save">
      <input key={'directory:'+choice.directory} id={id+'-directory'} defaultValue={choice.directory} disabled={busy}
        onBlur={event=>commit('directory',event)} />
      <Button variant="outline" size="icon-sm" aria-label={id==='bitmap-save'?'Browse bitmap save directory':'Browse save directory'} disabled={busy} onClick={onBrowse}><FolderOpen /></Button>
    </div></div>
    <div className="field"><label htmlFor={id+'-filename'}>Filename</label><div className="quick-save-name">
      <input key={'filename:'+choice.filename} id={id+'-filename'} defaultValue={choice.filename} disabled={busy}
        onBlur={event=>commit('filename',event)}
        onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();event.currentTarget.blur();save(event);}}} />
      <span>.{choice.format}</span>
    </div></div>
    <div className="quick-save-actions"><Button disabled={busy} onClick={save}><Save />Save</Button>
      {savedFile && <span role="status" title={savedFile}>Saved</span>}</div>
  </>;
}

function BitmapExport({choice,size,busy,onChoice,onSize,onViewScale,onDownload,onSave,onBrowse,savedFile}) {
  const commit=async(field,event)=>{
    const input=event.currentTarget,text=input.value;
    const next=await onSize({command:'commit',field,text});
    // A rejected or normalized edit can retain the prior value. Reset the DOM
    // field even when React's presentation key therefore stays unchanged.
    if(next && input.isConnected)input.value=String(next[field]);
  };
  return <details className="quick-save bitmap-export" open={choice.expanded} onToggle={event=>{
    if(!busy && event.currentTarget.open!==choice.expanded)onChoice({expanded:event.currentTarget.open});
  }}><summary onClick={event=>{if(busy)event.preventDefault();}}>Bitmap Export</summary>
    <MenuSelect label="Bitmap format" value={choice.format} options={['bmp','jpg','png','tif'].map(format=>[format,format.toUpperCase()])}
      disabled={busy} onChange={format=>onChoice({format})} />
    {['width','height'].map(field=><div className="bitmap-dimension" key={field}>
      <label htmlFor={'bitmap-'+field}>{field==='width'?'Width':'Height'}</label>
      <input id={'bitmap-'+field} key={size[field]} defaultValue={size[field]} inputMode="numeric" disabled={busy}
        onBlur={event=>commit(field,event)} onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();event.currentTarget.blur();}}} />
      <span>px</span>
      {[[-1,'large',ChevronsDown],[-1,'small',ChevronDown],[1,'small',ChevronUp],[1,'large',ChevronsUp]].map(([direction,step,Icon])=>
        <Button key={direction+step} type="button" variant="ghost" size="icon-sm" disabled={busy}
          aria-label={`${direction===1?'Increase':'Decrease'} bitmap ${field}, ${step} step`}
          onClick={()=>onSize({command:'step',field,direction,size:step})}><Icon /></Button>)}
    </div>)}
    <div className="bitmap-dimension"><label htmlFor="bitmap-percent">Scale</label>
      <input id="bitmap-percent" key={size.percent} defaultValue={size.percent} inputMode="decimal" disabled={busy}
        onBlur={event=>commit('percent',event)} onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();event.currentTarget.blur();}}} /><span>%</span>
    </div>
    <div className="bitmap-scale-actions"><Button type="button" variant="outline" size="icon-sm" aria-label="Use view scale" title="Use view scale" disabled={busy} onClick={onViewScale}><Scan /></Button>
      {bitmapScaleButtons.map(value=><Button key={value} type="button" variant="outline" size="sm" disabled={busy}
        onClick={()=>onSize({command:'preset',value})}>x{value}</Button>)}
      <Select value="" onValueChange={value=>onSize({command:'preset',value:Number(value)})} disabled={busy}>
        <SelectTrigger aria-label="More bitmap scales"><SelectValue placeholder="More" /></SelectTrigger>
        <SelectContent>{bitmapScaleMore.map(value=><SelectItem key={value} value={String(value)}>x{value}</SelectItem>)}</SelectContent>
      </Select>
    </div>
    <SaveLocation id="bitmap-save" choice={choice} busy={busy} onChange={onChoice} onBrowse={onBrowse} onSave={onSave} savedFile={savedFile} />
    <Button className="w-full" variant="outline" disabled={busy} onClick={onDownload}><Download />Download bitmap</Button>
  </details>;
}

export function NumericControl({ label, value, onChange, disabled }) {
  return <div className="field"><div className="field-heading"><label>{label}</label><output>{value}</output></div>
    <Slider aria-label={label} value={[value]} min={1} max={12} step={1} disabled={disabled} onValueChange={([value]) => onChange(value)} />
  </div>;
}

export function CheckControl({ label, checked, onChange, disabled }) {
  return <label className="check-field"><Checkbox checked={checked} onCheckedChange={onChange} disabled={disabled} />{label}</label>;
}

export function SavedProfiles({names,busy,onReplay,onSave,onSelect,onManage,name,onNameChange}) {
  const [editing,setEditing] = useState(null),cancel = useRef(null),label = useRef(null);
  const dialogOpener = useRef(null),summary = useRef(null);
  const save = () => {if (name !== '' && !busy) onSave(name);};
  const commit = () => {
    if (busy || !editing) return;
    if (editing.command === 'rename' && editing.stage !== 'replace') {
      if (editing.newName === '' || editing.newName === editing.name) {setEditing(null);return;}
      if (names.includes(editing.newName)) {setEditing({...editing,stage:'replace'});return;}
    }
    const {command,name,newName,stage} = editing;
    // A committed operation can remove its row, so return to the stable list
    // heading. Cancelling instead returns to the exact button that opened it.
    dialogOpener.current = summary.current;
    setEditing(null);onManage({command,name,...(command === 'rename' ? {newName,force:stage === 'replace'} : {})});
  };
  return <details className="saved-profiles" open><summary ref={summary}>Saved settings</summary>
    {onSave && <div className="profile-save"><input aria-label="Settings name" placeholder="Settings name" value={name} disabled={busy}
      onChange={event=>onNameChange(event.target.value)} onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();save();}}} />
      <Button type="button" variant="outline" size="icon-sm" aria-label="Save settings" disabled={busy || name===''} onClick={save}><Save /></Button></div>}
    <ul>{names.map(name=><li key={name}><span title={name}>{name === '' ? '(Empty name)' : name}</span>
      {onReplay && <><Button type="button" variant="ghost" size="icon-sm" aria-label={`Step through settings ${name}`} disabled={busy} onClick={()=>onReplay('play',name)}><Play /></Button>
        <Button type="button" variant="ghost" size="icon-sm" aria-label={`Vectorize with settings ${name}`} disabled={busy} onClick={()=>onReplay('fast-forward',name)}><FastForward /></Button></>}
      {onSelect && <Button type="button" variant="ghost" size="sm" aria-label={`Use settings ${name}`} disabled={busy} onClick={()=>onSelect(name)}>Use</Button>}
      {onManage && <><Button type="button" variant="ghost" size="icon-sm" aria-label={`Rename settings ${name}`} disabled={busy}
        onClick={event=>{dialogOpener.current=event.currentTarget;setEditing({command:'rename',name,newName:name});}}><Pencil /></Button>
        <Button type="button" variant="ghost" size="icon-sm" aria-label={`Delete settings ${name}`} disabled={busy}
          onClick={event=>{dialogOpener.current=event.currentTarget;setEditing({command:'delete',name});}}><Trash2 /></Button></>}
    </li>)}</ul>
    <Dialog open={Boolean(editing)} onOpenChange={open=>{if(!open)setEditing(null);}}>
      <DialogContent key={editing?.stage ?? editing?.command} showCloseButton={false} onOpenAutoFocus={event=>{
        event.preventDefault();if (editing?.command === 'rename' && editing.stage !== 'replace') label.current?.select();else cancel.current?.focus();
      }} onCloseAutoFocus={event=>{event.preventDefault();dialogOpener.current?.focus();}}>
        <DialogHeader><DialogTitle>{editing?.command === 'delete' ? 'Delete settings' : editing?.stage === 'replace' ? 'Overwrite Settings' : 'Change Settings Label'}</DialogTitle>
          <DialogDescription>{editing?.command === 'delete' ? `Delete the saved settings “${editing.name}”?` : editing?.stage === 'replace'
            ? `Overwrite the saved settings “${editing.newName}” with “${editing.name}”?` : 'Choose a new label for these saved settings.'}</DialogDescription></DialogHeader>
        <form onSubmit={event=>{event.preventDefault();commit();}}>
          {editing?.command === 'rename' && editing.stage !== 'replace' && <div className="profile-save"><input ref={label} aria-label="New settings name"
            value={editing.newName} disabled={busy} onChange={event=>setEditing({...editing,newName:event.target.value})} /></div>}
          <div className="confirmation-actions"><Button ref={cancel} type="button" variant="outline" onClick={()=>setEditing(null)}>Cancel</Button>
            <Button type="submit" disabled={busy} variant={editing?.command === 'delete' ? 'destructive' : 'default'}>
              {editing?.command === 'delete' ? 'Delete settings' : editing?.stage === 'replace' ? 'Overwrite settings' : 'Rename settings'}</Button></div>
        </form>
      </DialogContent>
    </Dialog>
  </details>;
}

const hexColor = color => '#' + [color[2], color[1], color[0]].map(value => Math.round(value * 255).toString(16).padStart(2, '0')).join('');
const bgraFromHex = (hex, alpha) => {
  const [red, green, blue] = hex.slice(1).match(/../g).map(value => Number.parseInt(value, 16) / 255);
  return [blue, green, red, alpha];
};

function ColorCount({value,onChange,disabled}) {
  const commit = event => {
    const parsed=Number(event.currentTarget.value);
    const next=Number.isInteger(parsed) ? Math.max(2,Math.min(24,parsed)) : value;
    event.currentTarget.value=String(next);onChange(next);
  };
  return <div className="field"><label htmlFor="palette-count">Number of colors</label><div className="count-control">
    {[[ -4,ChevronsDown ],[ -1,ChevronDown ]].map(([step,Icon])=><Button key={step} variant="ghost" size="icon-sm" aria-label={`Decrease colors by ${-step}`} disabled={disabled || value===2} onClick={()=>onChange(Math.max(2,value+step))}><Icon /></Button>)}
    <input key={value} id="palette-count" type="number" min="2" max="24" step="1" defaultValue={value} disabled={disabled} onBlur={commit}
      onKeyDown={event=>{if(event.key==='Enter'){event.preventDefault();commit(event);}else if(['PageUp','PageDown'].includes(event.key)){event.preventDefault();onChange(Math.max(2,Math.min(24,value+(event.key==='PageUp'?4:-4))));}}} />
    {[[ 1,ChevronUp ],[ 4,ChevronsUp ]].map(([step,Icon])=><Button key={step} variant="ghost" size="icon-sm" aria-label={`Increase colors by ${step}`} disabled={disabled || value===24} onClick={()=>onChange(Math.min(24,value+step))}><Icon /></Button>)}
  </div></div>;
}

function PaletteControls({ document, settings, set, busy, onAnalyze, paletteState,onPaletteAction,onCountChange }) {
  const colors = Array.from({ length: settings.palette.length / 4 }, (_, index) => settings.palette.slice(index * 4, index * 4 + 4));
  return <>
    {settings.advanced && <><MenuSelect label="Color mode" value={settings.paletteMode} disabled={busy}
      options={[[2, 'Flexible palette'], [1, 'Fixed palette']]} onChange={value => set('paletteMode', Number(value))} />
    </>}
    {settings.paletteMode !== 2 && <>
      <div className="field"><div className="field-heading"><label>Selected palette</label><span>{colors.length} colors</span></div>
        <div className="color-swatches">{colors.map((color, index) => <div className="palette-color" key={index}><label title={`Color ${index + 1}`}>
          <input type="color" aria-label={`Palette color ${index + 1}`} disabled={busy} value={hexColor(color)} onChange={event => {
            onPaletteAction('color',index,bgraFromHex(event.target.value,color[3]));
          }} /></label><Button variant="ghost" size="icon-sm" aria-label={`Delete palette color ${index+1}`} disabled={busy} onClick={()=>onPaletteAction('delete',index)}><Trash2 /></Button></div>)}</div>
        <div className="palette-actions"><Button variant="ghost" size="sm" disabled={busy || colors.length>=25} onClick={()=>onPaletteAction('add')}><Plus />Add color</Button>
          <Button variant="ghost" size="sm" disabled={busy || !paletteState.customized} onClick={()=>onPaletteAction('revert')}><RotateCcw />Revert palette</Button></div>
      </div>
      <div className="field"><div className="field-heading"><label>Quick palettes</label><Button variant="ghost" size="sm" disabled={busy} onClick={onAnalyze}>Recalculate</Button></div>
        <div className="quick-palettes">{document.palettes.map(candidate => <button type="button" key={candidate.count} disabled={busy}
          aria-label={`${candidate.count} color palette`} aria-pressed={candidate.colors.length===colors.length && quantizeQColorChannels(candidate.colors.flat()).every((value,index)=>value===settings.palette[index])}
          onClick={() => onPaletteAction('choose',candidate.colors.flat())}>
          <span>{candidate.count}</span><span className="palette-strip">{candidate.colors.map((color, index) => <i key={index} style={{ background: hexColor(color) }} />)}</span>
        </button>)}</div>
      </div>
      {settings.advanced && <><ColorCount value={paletteState.count} onChange={onCountChange} disabled={busy} />
        <MenuSelect label="Color sensitivity" value={settings.sensitivity} options={[[0,'High'],[1,'Medium'],[2,'Low']]} disabled={busy} onChange={value=>set('sensitivity',Number(value))} />
        <CheckControl label="Image has blended edges" checked={settings.antiAlias} disabled={busy} onChange={value=>set('antiAlias',value)} /></>}
    </>}
  </>;
}

function WizardPage({ page, document, settings, set, busy, dirty, select, onOpen, onAnalyze, exportOptions, setExportOptions, onExport,
    onEdit,onBackground,editTool,onToolChange,editColor,onColorChange,paletteState,onPaletteAction,onCountChange,profileNames,onReplayProfile,onSaveProfile,onManageProfile,profileName,onProfileName,sourceHistory,onReopenSource,onBrowseDirectory,onRevealDirectory,
    onReview,skipReview,onSkipReviewChange,onDoneReview,onImprove,quickSave,onQuickSaveChange,onQuickSave,onQuickSaveBrowse,savedFile,
    bitmapSave,onBitmapSaveChange,onBitmapSize,onBitmapViewScale,onBitmapDownload,onBitmapSave,onBitmapBrowse,bitmapSavedFile }) {
  switch (page) {
    case 'load': return <><div className="load-content"><Upload size={28} strokeWidth={1.5} /><p>Open a bitmap to begin.</p>
      <Button className="w-full" onClick={onOpen} disabled={busy}><Upload />Browse for image</Button><p className="help-text">PNG, BMP, JPEG, GIF and TIFF files. Processing stays on your computer.</p></div>
      <details className="recent-directories"><summary>Recent directories</summary>
        <ul>{sourceHistory.directoryHistory.directories.map((directory,index)=><li key={directory}>
          <span title={directory}>{directory.split(/[\\/]/).at(-1)||directory}</span>
          <Button type="button" variant="ghost" size="icon-sm" disabled={busy} title={`Browse ${directory}`}
            aria-label={`Browse recent directory ${directory}`} onClick={()=>onBrowseDirectory(index)}><FolderOpen /></Button>
          <Button type="button" variant="ghost" size="icon-sm" disabled={busy} title={`Open folder ${directory}`}
            aria-label={`Open recent directory ${directory}`} onClick={()=>onRevealDirectory(index)}><ExternalLink /></Button>
        </li>)}</ul>
      </details>
      <details className="recent-files"><summary>Recent files</summary>
        <ul>{sourceHistory.files.map((entry,index)=><li key={entry.filename}>
          <Button type="button" variant="ghost" className="recent-file" title={entry.filename} disabled={busy}
            aria-label={`Open recent image ${entry.filename}`} onClick={()=>onReopenSource(index)}>
            <span className="recent-thumbnail" aria-hidden="true">{entry.thumbnail && <Bitmap bitmap={entry.thumbnail} label={null} />}</span>
            <span className="recent-label"><span>{entry.filename.split(/[\\/]/).at(-1)}</span><small>{entry.description}</small></span>
          </Button>
        </li>)}</ul>
      </details></>;
    case 'mode': return <>
      <p className="page-intro">Choose how much control you want.</p>
      <Choice title="Fully automatic" description="Detect settings and vectorize." icon={Sparkles} disabled={busy} onClick={() => select('mode', 'auto')} />
      <Choice title="Basic" description="Guided choices for your image." icon={Play} disabled={busy} onClick={() => select('mode', 'basic')} />
      <Choice title="Advanced" description="Fine-tune each processing step." icon={SlidersHorizontal} disabled={busy} onClick={() => select('mode', 'advanced')} />
      <SavedProfiles names={profileNames} busy={busy} onReplay={onReplayProfile} />
    </>;
    case 'type': return <><p className="page-intro">What kind of image is this?</p>
      {[[0, 'Photograph', 'Continuous colors and natural detail.'], [1, 'Artwork with blended edges', 'Logos and illustrations with smooth edges.'],
        [2, 'Artwork without blending', 'Pixel art and hard color boundaries.']].map(([value, title, description]) =>
        <Choice key={value} title={title} description={description} selected={settings.category === value} disabled={busy} onClick={() => select('category', value)} />)}
    </>;
    case 'transparency': return <><p className="page-intro">Keep transparency or choose a background.</p>
      <Choice title="Leave transparent" selected={settings.backgroundMode === 0} disabled={busy} onClick={() => select('backgroundMode', 0)} />
      <Choice title="Flatten to white" selected={settings.backgroundMode === 1} disabled={busy} onClick={() => select('backgroundMode', 1)} />
      <Choice title="Flatten to a custom color" selected={settings.backgroundMode === 2} disabled={busy} onClick={() => select('backgroundMode', 2)} />
      <div className="field"><label htmlFor="background-color">Custom background</label><input id="background-color" type="color" disabled={busy}
        value={hexColor(settings.background)} onChange={event => set('background', bgraFromHex(event.target.value, 1))} /></div>
    </>;
    case 'quality': return <><p className="page-intro">{settings.category === 0 ? 'Choose the amount of detail to keep.' : 'How clean are the edges in your image?'}</p>
      {['High', 'Medium', 'Low'].map((title, value) => <Choice key={value} title={title} selected={settings.quality === value}
        disabled={busy} onClick={() => select('quality', value)} />)}
    </>;
    case 'colors': return <><p className="page-intro">Choose how to find the output colors.</p>
      {[[0, 'Two colors', 'Choose a two-color palette automatically.'], [1, 'Fixed palette', 'Select and edit the colors to use.'],
        [2, 'Flexible palette', 'Let the engine find colors in the image.']].map(([value, title, description]) =>
        <Choice key={value} title={title} description={description} selected={settings.paletteMode === value}
          disabled={busy} onClick={() => select('paletteMode', value)} />)}
    </>;
    case 'palette': return <PaletteControls {...{ document, settings, set, busy, onAnalyze,paletteState,onPaletteAction,onCountChange }} />;
    case 'segmentation': return <><p className="page-intro">Control the shapes found in your image.</p>
      <NumericControl label="Complexity" value={settings.complexity} disabled={busy} onChange={value => set('complexity', value)} />
      <div className="field"><label htmlFor="minimum-pixels">Minimum pixels per region</label><input id="minimum-pixels" type="number" min="0" step="1"
        value={settings.minimumPixels} disabled={busy} onChange={event => set('minimumPixels', event.target.value === '' ? 0 : Number(event.target.value))} /></div>
      <MenuSelect label="Artifact rejection" value={settings.artifactRejection} options={[[0, 'None'], [1, 'Moderate'], [2, 'Aggressive']]}
        disabled={busy} onChange={value => set('artifactRejection', Number(value))} />
      <CheckControl label="Cluster colors" checked={settings.clusterColors} disabled={busy} onChange={value => set('clusterColors', value)} />
      <p className="help-text">{document.regionCount} regions</p>
      <Button variant="outline" disabled={busy || dirty} onClick={() => onEdit('begin')}><Layers />Edit segmentation</Button>
    </>;
    case 'editing': return <>
      <div className="editing-tools" role="group" aria-label="Segmentation tools">
        {[[ 'pan','Pan',Hand ],[ 'pencil','Pencil',Pencil ],[ 'fill','Fill',PaintBucket ],[ 'zap','Zap',Zap ],[ 'picker','Pick color',Pipette ]].map(([tool,label,Icon]) =>
          <Button key={tool} variant="ghost" aria-pressed={editTool === tool} disabled={busy} onClick={() => onToolChange(tool)}><Icon />{label}</Button>)}
      </div>
      <div className="field"><label htmlFor="edit-color">Paint color</label><input id="edit-color" type="color" disabled={busy}
        value={hexColor(editColor)} onChange={event => onColorChange(bgraFromHex(event.target.value,editColor[3]))} /></div>
      <div className="field"><label htmlFor="edit-alpha">Opacity</label><input id="edit-alpha" type="number" min="0" max="1" step="0.01"
        value={editColor[3]} disabled={busy} onChange={event => {
          const alpha = Number(event.target.value);if (Number.isFinite(alpha) && alpha >= 0 && alpha <= 1) onColorChange([...editColor.slice(0,3),alpha]);
        }} /></div>
      <div className="editing-history" role="group" aria-label="Edit history">
        <Button variant="outline" disabled={busy || !document.editor?.undoCount} onClick={() => onEdit('undo')}><Undo2 />Undo</Button>
        <Button variant="outline" disabled={busy || !document.editor?.redoCount} onClick={() => onEdit('redo')}><Redo2 />Redo</Button>
        <Button variant="ghost" disabled={busy || !document.editor?.undoCount} onClick={() => onEdit('revert')}><RotateCcw />Revert</Button>
      </div>
      <p className="help-text">{editTool === 'pencil' ? 'Drag to paint.' : editTool === 'fill' ? 'Click a color region to fill it.' : editTool === 'zap' ? 'Click a region to replace it with neighboring colors.' : editTool === 'picker' ? 'Release over a segment color to select it.' : 'Drag to move the image.'} Hold Shift to pan. {settings.advanced ? 'Next commits edits before path smoothing.' : 'Next updates the vector.'}</p>
    </>;
    case 'background': return <>
      <p className="page-intro">Click shapes to remove or restore them.</p>
      <div className="editing-tools" role="group" aria-label="Background tools">
        {[[ 'pan','Pan',Hand ],[ 'background','Erase',Eraser ]].map(([tool,label,Icon]) =>
          <Button key={tool} variant="ghost" aria-pressed={editTool === tool} disabled={busy} onClick={() => onToolChange(tool)}><Icon />{label}</Button>)}
      </div>
      <div className="editing-history" role="group" aria-label="Background history">
        <Button variant="outline" disabled={busy || !document.background.undo.length} onClick={() => onBackground('undo')}><Undo2 />Undo</Button>
        <Button variant="outline" disabled={busy || !document.background.redo.length} onClick={() => onBackground('redo')}><Redo2 />Redo</Button>
        <Button variant="ghost" disabled={busy || !document.background.undo.length} onClick={() => onBackground('revert')}><RotateCcw />Revert</Button>
      </div>
      <p className="help-text">Hold Shift to pan. Removed shapes have red dashed outlines and stay out of exports. Processing the vector again clears removals.</p>
    </>;
    case 'smoothing': return <><p className="page-intro">Refine the paths and fitted curves.</p>
      <NumericControl label="Smoothness" value={settings.smoothness} disabled={busy} onChange={value => set('smoothness', value)} />
      <NumericControl label="Curve complexity" value={settings.curveComplexity} disabled={busy} onChange={value => set('curveComplexity', value)} />
      <CheckControl label="Detect sharp corners" checked={settings.detectCorners} disabled={busy} onChange={value => set('detectCorners', value)} />
      <CheckControl label="Anti-alias contours" checked={settings.smoothingAntiAlias} disabled={busy} onChange={value => set('smoothingAntiAlias', value)} />
      <p className="help-text">{document.curveCount} fitted curves</p>
    </>;
    case 'review': return <>
      <Button className="w-full review-done" disabled={busy} onClick={onDoneReview}>Done reviewing<ArrowRight /></Button>
      <div className="field"><label id="review-detail-label">Detail level</label>
        <div className="review-options" role="group" aria-labelledby="review-detail-label">
          {['High','Medium','Low'].map((label,quality)=><Button key={label} variant="outline" disabled={busy}
            aria-pressed={settings.quality===quality} onClick={()=>onReview(label.toLowerCase())}>{label}</Button>)}
        </div>
      </div>
      <div className="field"><label id="review-colors-label">Colors</label>
        <div className="review-options" role="group" aria-labelledby="review-colors-label">
          <Button variant="outline" disabled={busy} aria-pressed={settings.paletteMode===2} onClick={()=>onReview('unlimited-colors')}>Unlimited colors</Button>
          <Button variant="outline" disabled={busy} aria-pressed={settings.paletteMode!==2} onClick={()=>onReview('custom-colors')}>Custom colors</Button>
        </div>
      </div>
      {settings.paletteMode!==2 && <div className="field"><div className="field-heading"><label>Current palette</label>
        <Button variant="ghost" size="sm" disabled={busy} onClick={()=>onReview('custom-colors')}>Change</Button></div>
        <span className="palette-strip" role="img" aria-label="Current palette">{Array.from({length:settings.palette.length/4},(_,index)=>{
          const color=hexColor(settings.palette.slice(index*4,index*4+4));
          return <i key={index} title={color} style={{background:color}} />;
        })}</span>
      </div>}
      <div className="review-actions">
        <Button variant="outline" disabled={busy} onClick={()=>onReview('background')}><Eraser />Remove background</Button>
        <Button variant="outline" disabled={busy} onClick={()=>onReview('edit')}><Pencil />Edit result</Button>
        <Button variant="outline" disabled={busy} onClick={()=>onReview('hand-pick')}><SlidersHorizontal />Hand-pick settings</Button>
      </div>
      <CheckControl label="Skip this page next time" checked={skipReview} disabled={busy} onChange={onSkipReviewChange} />
    </>;
    case 'export': return <><p className="page-intro">Save your vector for use anywhere.</p>
      <MenuSelect label="File format" value={exportOptions.format} options={['ai', 'dxf', 'emf', 'eps', 'pdf', 'svg'].map(value => [value, value.toUpperCase()])}
        disabled={busy} onChange={format => setExportOptions({format})} />
      <MenuSelect label="Shape layout" value={exportOptions.layout} options={[[ 'stacked', 'Stacked shapes'], ['cutouts', 'Cut out shapes'], ['grouped-cutouts', 'Grouped cutouts']]}
        disabled={busy} onChange={layout => setExportOptions({layout})} />
      <CheckControl label="Thin stroke" checked={exportOptions.thinStroke} disabled={busy} onChange={thinStroke => setExportOptions({thinStroke})} />
      <CheckControl label="Compact EPS" checked={exportOptions.compact} disabled={busy || exportOptions.format !== 'eps'} onChange={compact => setExportOptions({compact})} />
      <MenuSelect label="DXF mode" value={exportOptions.curveMode} disabled={busy || exportOptions.format !== 'dxf'}
        options={[[ 'splines','Splines and lines'],['sparse-lines','Lines, sparse'],['dense-lines','Lines, dense']]}
        onChange={curveMode => setExportOptions({curveMode})} />
      <details className="quick-save" open={quickSave.expanded} onToggle={event=>{
        if(!busy && event.currentTarget.open!==quickSave.expanded)onQuickSaveChange({expanded:event.currentTarget.open});
      }}><summary onClick={event=>{if(busy)event.preventDefault();}}>Quick Save</summary>
        <SaveLocation id="quick-save" choice={quickSave} busy={busy || !document.svg} onChange={onQuickSaveChange}
          onBrowse={onQuickSaveBrowse} onSave={onQuickSave} savedFile={savedFile} />
      </details>
      <Button className="w-full" variant="outline" disabled={busy || !document.svg} onClick={onExport}><Download />Download {exportOptions.format.toUpperCase()}</Button>
      <BitmapExport choice={bitmapSave} size={document.bitmapSize[settings.advanced?'advanced':'basic']} busy={busy || !document.svg}
        onChoice={onBitmapSaveChange} onSize={onBitmapSize} onViewScale={onBitmapViewScale} onDownload={onBitmapDownload}
        onSave={onBitmapSave} onBrowse={onBitmapBrowse} savedFile={bitmapSavedFile} />
      {!settings.advanced && <Button variant="ghost" disabled={busy} onClick={onImprove}>Improve result</Button>}
      {settings.advanced && <SavedProfiles names={profileNames} busy={busy} onSave={onSaveProfile} onManage={onManageProfile} name={profileName} onNameChange={onProfileName} />}
    </>;
    default: throw new Error('Unknown wizard page.');
  }
}

export function Wizard({ page, dirty, revert, busy, status, onBack, onNext, onStart, canNext,automatic,onAutomaticChange, ...props }) {
  return <aside className="wizard" aria-label="Vectorization wizard">
    <div className="wizard-heading"><span className="wizard-step"><Palette size={14} />{props.settings?.advanced ? 'Advanced wizard' : 'Basic wizard'}</span>
      <h2 tabIndex={-1} id="wizard-title">{pageTitles[page]}</h2></div>
    <fieldset className={`wizard-content${page==='review' ? ' review-content' : ''}`} disabled={busy}><WizardPage {...props} page={page} busy={busy} dirty={dirty} />
      {props.settings?.advanced && automatic && (page !== 'palette' || props.settings.paletteMode !== 2) &&
        <CheckControl label={`Auto-Update ${page === 'palette' ? 'Palette' : page === 'segmentation' ? 'Segmentation' : 'Vectorization'}`}
          checked={automatic.checked} disabled={busy || !automatic.enabled} onChange={onAutomaticChange} />}
    </fieldset>
    <div className="wizard-bottom">
      <p className={`operation-status ${status.kind === 'error' ? 'error' : ''}`} role={status.kind === 'error' ? 'alert' : 'status'}>{status.message}</p>
      <div className="wizard-navigation">
        <Button variant="ghost" size="sm" disabled={busy || (page === 'load' && !props.document)} onClick={onStart}><RotateCcw size={14} />{page === 'load' ? 'Reset' : 'Start'}</Button>
        <Button variant="outline" size="sm" disabled={busy || page === 'load' || page === 'mode'} onClick={onBack}><ArrowLeft size={14} />{revert ? 'Revert' : 'Back'}</Button>
        <Button size="sm" disabled={busy || !canNext} onClick={onNext}>{dirty ? 'Update' : 'Next'}<ArrowRight size={14} /></Button>
      </div>
    </div>
  </aside>;
}
