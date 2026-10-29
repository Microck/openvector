import { useEffect, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, Check, FileImage, FolderOpen, Images, Plus, RotateCcw, Square, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { MenuSelect, NumericControl, CheckControl, SavedProfiles } from './wizard';
import { request } from './api.mjs';
import {activeProcessingSteps as activeSteps,completedProcessingSteps as completedSteps} from '../processing-job.mjs';

const basename = filename => filename.split(/[\\/]/).at(-1);
const titles = {files:'Files to process',settings:'Batch settings',progress:'Processing images',summary:'Batch summary'};

function Thumbnail({image}) {
  const canvas = useRef(null);
  useEffect(() => {
    if (!image?.id) return;
    const controller = new AbortController();
    request('batch-thumbnail?id='+image.id,undefined,{signal:controller.signal}).then(pixels => {
      const rgba = Uint8ClampedArray.from(atob(pixels.rgba),value => value.charCodeAt(0));
      canvas.current?.getContext('2d').putImageData(new ImageData(rgba,pixels.width,pixels.height),0,0);
    }).catch(error => {if (error.name !== 'AbortError') console.error(error);});
    return () => controller.abort();
  },[image?.id]);
  return image?.id ? <canvas ref={canvas} width={image.width} height={image.height} aria-hidden="true" /> : <FileImage aria-hidden="true" />;
}

function Settings({state,configure,folder}) {
  const {settings,queue,exportOptions} = state;
  const set = (key,value) => configure({settings:{[key]:value}});
  return <>
    <CheckControl label="Advanced controls" checked={settings.advanced} onChange={value => set('advanced',value)} />
    {!settings.advanced && <>
      <MenuSelect label="Image type" value={settings.category} options={[[3,'Auto-detect'],[0,'Photograph'],[1,'Artwork with blended edges'],[2,'Artwork without blending']]} onChange={value => set('category',Number(value))} />
      <MenuSelect label="Image quality" value={settings.quality} options={[[3,'Auto-detect'],[0,'High'],[1,'Medium'],[2,'Low']]} onChange={value => set('quality',Number(value))} />
    </>}
    <MenuSelect label="Color mode" value={settings.paletteMode} options={[[3,'Auto-detect'],[1,'Custom colors'],[2,'Unlimited colors']]} onChange={value => set('paletteMode',Number(value))} />
    <MenuSelect label="Number of colors" value={settings.colorCount} disabled={settings.paletteMode !== 1}
      options={[...(settings.advanced ? [] : [[0,'Auto-detect']]),...Array.from({length:settings.advanced ? 23 : 11},(_,index) => [index+2,String(index+2)])]}
      onChange={value => set('colorCount',Number(value))} />
    <MenuSelect label="Transparency" value={settings.backgroundMode} options={[[1,'Flatten to white'],[0,'Preserve transparency'],[3,'Auto-detect']]} onChange={value => set('backgroundMode',Number(value))} />
    {settings.advanced && <details className="batch-details"><summary>Processing controls</summary>
      <MenuSelect label="Color sensitivity" value={settings.sensitivity} options={[[0,'High'],[1,'Medium'],[2,'Low']]} onChange={value => set('sensitivity',Number(value))} />
      <CheckControl label="Image has blended edges" checked={settings.antiAlias} onChange={value => set('antiAlias',value)} />
      <NumericControl label="Complexity" value={settings.complexity} onChange={value => set('complexity',value)} />
      <div className="field"><label htmlFor="batch-minimum-pixels">Minimum pixels per region</label><input id="batch-minimum-pixels" name="minimumPixels" type="number" min="0" max="2147483647" defaultValue={settings.minimumPixels} key={settings.minimumPixels} /></div>
      <MenuSelect label="Artifact rejection" value={settings.artifactRejection} options={[[0,'None'],[1,'Moderate'],[2,'Aggressive']]} onChange={value => set('artifactRejection',Number(value))} />
      <CheckControl label="Cluster colors" checked={settings.clusterColors} onChange={value => set('clusterColors',value)} />
      <NumericControl label="Smoothness" value={settings.smoothness} onChange={value => set('smoothness',value)} />
      <NumericControl label="Curve complexity" value={settings.curveComplexity} onChange={value => set('curveComplexity',value)} />
      <CheckControl label="Detect sharp corners" checked={settings.detectCorners} onChange={value => set('detectCorners',value)} />
      <CheckControl label="Anti-alias contours" checked={settings.smoothingAntiAlias} onChange={value => set('smoothingAntiAlias',value)} />
    </details>}
    <div className="field"><label htmlFor="batch-export-folder">Export folder</label><div className="batch-folder">
      <input ref={folder} id="batch-export-folder" name="exportDirectory" defaultValue={queue.exportDirectory} key={queue.exportDirectory} />
      <Button type="button" variant="outline" size="icon" aria-label="Browse for export folder" onClick={() => configure(null,'pick-folder')}><FolderOpen /></Button>
    </div></div>
    <MenuSelect label="File format" value={queue.exportFormat} options={['ai','svg','eps','pdf','dxf'].map(value => [value,value.toUpperCase()])} onChange={exportFormat => configure({exportFormat})} />
    <MenuSelect label="Shape mode" value={exportOptions.layout} options={[[ 'stacked','Stacked shapes'],['cutouts','Cut out shapes'],['grouped-cutouts','Grouped cutouts']]} onChange={layout => configure({exportOptions:{layout}})} />
    <CheckControl label="Thin outline" checked={exportOptions.thinStroke} onChange={thinStroke => configure({exportOptions:{thinStroke}})} />
    <CheckControl label="Compact EPS" checked={exportOptions.compact} disabled={queue.exportFormat !== 'eps'}
      onChange={compact => configure({exportOptions:{compact}})} />
    <MenuSelect label="DXF mode" value={exportOptions.curveMode} disabled={queue.exportFormat !== 'dxf'}
      options={[[ 'splines','Splines and lines'],['sparse-lines','Lines, sparse'],['dense-lines','Lines, dense']]}
      onChange={curveMode => configure({exportOptions:{curveMode}})} />
  </>;
}

export function BatchWorkspace({initial,brand,onSingle,profileNames,onProfileNames}) {
  const [state,setState] = useState(initial),[pending,setPending] = useState(null),[error,setError] = useState('');
  const folder = useRef(null),cancel = useRef(null),commands = useRef(Promise.resolve());
  const accept = next => setState(previous => next.revision >= previous.revision ? next : previous);
  const send = input => {
    const operation = commands.current.then(async () => {
      setPending(input.command);setError('');
      try {const next = await request('batch',input);accept(next);return next;}
      catch (failure) {setError(failure.message);return null;}
      finally {setPending(null);}
    });
    commands.current = operation;return operation;
  };
  const selectProfile = name => {
    const operation = commands.current.then(async()=>{
      setPending('profile');setError('');
      try {
        const next = await request('profiles',{command:'select',name});
        onProfileNames(next.names);accept(next.batch);
        // This input keeps a local form draft. Selecting a profile discards
        // that draft even when the committed value has not changed.
        const minimum = folder.current?.form.elements.namedItem('minimumPixels');
        if (minimum) minimum.value = String(next.batch.settings.minimumPixels);
      } catch (failure) {setError(failure.message);}
      finally {setPending(null);}
    });
    commands.current = operation;return operation;
  };
  const configure = async (patch,after) => {
    const minimum = folder.current?.form.elements.namedItem('minimumPixels');
    const next = await send({command:'configure',exportDirectory:folder.current?.value ?? state.queue.exportDirectory,...patch,
      settings:{...(minimum ? {minimumPixels:Number(minimum.value)} : {}),...patch?.settings}});
    if (next && after) return send({command:after});
    return next;
  };
  const navigate = async page => {
    if (state.page === 'settings' && !await configure({})) return;
    await send({command:'page',page});
  };
  const single = async () => {
    if (state.page === 'settings' && !await configure({})) return;
    const next = await send({command:'leave'});if (next) onSingle(next);
  };
  useEffect(() => {
    if (!state.job.running) return;
    const controller = new AbortController();let timer;
    const poll = async () => {
      try {
        const next = await request('batch',undefined,{signal:controller.signal});accept(next);
        if (next.job.running) timer = setTimeout(poll,150);
      } catch (failure) {if (failure.name !== 'AbortError') setError(failure.message);}
    };
    timer = setTimeout(poll,150);
    return () => {clearTimeout(timer);controller.abort();};
  },[state.job.running]);
  useEffect(() => {window.document.getElementById('batch-title')?.focus({preventScroll:true});},[state.page]);
  const busy = Boolean(pending || state.job.running);
  const selected = state.queue.queue.filter(record => record.selected).length;
  const images = new Map(state.thumbnails.map(image => [image.filename,image]));
  const current = state.job.current,summary = state.job.summary;
  const taskLabel = current && (current.complete ? completedSteps[current.stage] : `${activeSteps[current.stage]}: ${current.progress}%`);
  const results = new Map(summary?.files.map(file => [file.filename,file]) ?? []);
  const status = error || state.job.error || (state.job.status === 'cancelling' ? 'Cancellation requested.' :
    state.job.status === 'no-files' ? 'No files to process. Existing files were skipped.' : state.job.status === 'cancelled' ? 'Batch cancelled.' : '');
  return <main className="application" aria-busy={busy}>
    <header className="app-header"><div className="brand">{brand}<span>openvector</span></div><span className="header-divider" />
      <div className="current-file"><Images size={14} /><span>Batch processing</span><span>{state.queue.queue.length} images</span></div>
      <div className="header-actions"><Button variant="ghost" size="sm" disabled={busy} onClick={single}><FileImage />Single image</Button></div>
    </header>
    <div className="workspace">
      <section className="batch-viewer" aria-label="Batch image queue">
        <div className="batch-toolbar"><Button size="sm" disabled={busy} onClick={() => send({command:'pick-files'})}><Plus />Add images</Button>
          <Button variant="ghost" size="sm" disabled={busy || !selected} onClick={() => send({command:'remove-selected'})}><Trash2 />Remove selected</Button>
          <Button variant="ghost" size="sm" disabled={busy || !state.queue.queue.length} onClick={() => send({command:'clear'})}>Clear all</Button>
          <span>{selected ? `${selected} selected` : `${state.queue.queue.length} images`}</span></div>
        {state.queue.queue.length ? <div className="batch-grid">{state.queue.queue.map((record,index) => {
          const image = images.get(record.filename),file = results.get(record.filename);
          return <button type="button" className="batch-image" key={record.filename} aria-pressed={record.selected} disabled={busy}
            aria-label={`Select ${basename(record.filename)}`} title={record.filename}
            onClick={() => send({command:'select',index,selected:!record.selected})}>
            <span className="batch-thumbnail"><Thumbnail image={image} />{record.selected && <Check className="batch-selected" />}</span>
            <strong>{basename(record.filename)}</strong><small>{image?.error ? 'Could not open image' : image ? `${image.sourceWidth} × ${image.sourceHeight}` : 'Reading image'}</small>
            {file && <small className={file.status.startsWith('failed') ? 'batch-file-error' : ''}>{file.status === 'written' ? 'Saved' : file.status === 'failed-load' ? 'Failed to open' : file.status === 'failed-save' ? 'Failed to save' : file.status === 'skipped' ? 'Skipped' : 'Not processed'}</small>}
          </button>;
        })}</div> : <div className="empty-view"><Images className="empty-mark" strokeWidth={.6} /><h1>Convert a batch of images</h1><p>Add images, choose shared settings, and export.</p>
          <Button disabled={busy} onClick={() => send({command:'pick-files'})}><Plus />Add images</Button></div>}
      </section>
      <aside className="wizard" aria-label="Batch wizard"><div className="wizard-heading"><span className="wizard-step"><Images size={14} />Batch processing</span>
        <h2 id="batch-title" tabIndex={-1}>{titles[state.page]}</h2></div>
        <form id="batch-options" className="wizard-content" onSubmit={event => {event.preventDefault();void configure({},'start');}}>
          <fieldset disabled={Boolean(state.job.running || pending && pending !== 'configure')} className="batch-fields">
            {state.page === 'files' && <><Button type="button" className="w-full" disabled={busy} onClick={() => send({command:'pick-files'})}><Plus />Add images</Button>
              <ul className="batch-file-list">{state.queue.queue.map((record,index) => <li key={record.filename} title={record.filename}>
                <label><Checkbox checked={record.selected} onCheckedChange={selected => send({command:'select',index,selected})} /><span>{basename(record.filename)}</span></label>
                <Button type="button" variant="ghost" size="icon-sm" aria-label={`Remove ${basename(record.filename)}`} onClick={() => send({command:'remove',index})}><Trash2 /></Button>
              </li>)}</ul></>}
            {state.page === 'settings' && <><SavedProfiles names={profileNames} busy={busy} onSelect={selectProfile} /><Settings {...{state,configure,folder}} /></>}
            {state.page === 'summary' && <><div className="review-stat"><span>Saved</span><strong>{summary.converted}</strong></div>
              <div className="review-stat"><span>Failed</span><strong>{summary.failed}</strong></div><div className="review-stat"><span>Skipped</span><strong>{summary.skipped}</strong></div>
              <Button type="button" variant="outline" className="batch-show-folder" onClick={() => send({command:'reveal-folder'})}><FolderOpen />Show export folder</Button>
              <p className="batch-output-path">{summary.queue.exportDirectory}</p></>}
          </fieldset>
          {state.page === 'progress' && <div className="batch-progress" role="status">
            <strong>{current ? `Image ${current.index+1} of ${state.queue.queue.length}` : 'Preparing batch'}</strong>
            {current && <><p>{basename(current.filename)}</p><span>{current.result ? (current.stage === 'load' ? 'Could not open image' : 'Could not save vector') : taskLabel}</span></>}
            {state.job.status === 'cancelling' && <p>Finishing the current task.</p>}
          </div>}
          {summary?.log && <details className="batch-details batch-log"><summary>Batch processing log</summary><pre>{summary.log}</pre></details>}
        </form>
        <div className="wizard-bottom"><p className={`operation-status ${error || state.job.error ? 'error' : ''}`} role={error || state.job.error ? 'alert' : 'status'}>{status}</p>
          <div className="wizard-navigation">
            {state.job.running ? <Button variant="outline" disabled={state.job.status === 'cancelling'} onClick={() => send({command:'cancel'})}><Square />Cancel</Button> : <>
              <Button variant="ghost" size="sm" disabled={pending} onClick={() => navigate('files')}><RotateCcw />Start</Button>
              <Button variant="outline" size="sm" disabled={pending || state.page === 'files'} onClick={() => navigate(state.page === 'summary' ? 'settings' : 'files')}><ArrowLeft />Back</Button>
              {state.page === 'files' ? <Button size="sm" disabled={pending} onClick={() => navigate('settings')}>Next<ArrowRight /></Button> :
                state.page === 'settings' ? <Button size="sm" type="submit" form="batch-options" disabled={pending}>Process<ArrowRight /></Button> :
                  <Button size="sm" disabled={pending} onClick={() => navigate('files')}>Done<ArrowRight /></Button>}
            </>}
          </div></div>
      </aside>
    </div>
    <Dialog open={Boolean(state.job.overwrite)} onOpenChange={open => {if (!open && state.job.overwrite) void send({command:'decide',decision:'cancel'});}}>
      <DialogContent showCloseButton={false} onInteractOutside={event => event.preventDefault()} onOpenAutoFocus={event => {event.preventDefault();cancel.current?.focus();}}>
        <DialogHeader><DialogTitle>Overwrite existing files?</DialogTitle><DialogDescription>Choose how to handle these output files.</DialogDescription></DialogHeader>
        <ul className="batch-overwrite-files">{state.job.overwrite?.map(filename => <li key={filename}>{filename}</li>)}</ul>
        <div className="confirmation-actions"><Button ref={cancel} variant="outline" disabled={pending} onClick={() => send({command:'decide',decision:'cancel'})}>Cancel</Button>
          <Button variant="outline" disabled={pending} onClick={() => send({command:'decide',decision:'skip'})}>Skip existing</Button>
          <Button disabled={pending} onClick={() => send({command:'decide',decision:'overwrite'})}>Overwrite</Button></div>
      </DialogContent>
    </Dialog>
    <Dialog open={state.duplicates.length > 0} onOpenChange={open => {if (!open) void send({command:'dismiss-duplicates'});}}>
      <DialogContent><DialogHeader><DialogTitle>Images already added</DialogTitle><DialogDescription>Each filename can appear once in the batch.</DialogDescription></DialogHeader>
        <ul className="batch-overwrite-files">{state.duplicates.map((filename,index) => <li key={index}>{filename}</li>)}</ul>
        <Button onClick={() => send({command:'dismiss-duplicates'})}>Close</Button></DialogContent>
    </Dialog>
    <footer className="app-footer"><span className="local-indicator" />Local processing<span className="footer-state">{titles[state.page]}</span><span>Development preview</span></footer>
  </main>;
}
