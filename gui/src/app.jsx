import { useCallback, useEffect, useRef, useState } from 'react';
import { Download, FileImage, Images, Info, Square, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { IconButton, Viewer } from './viewer';
import { pageTitles, Wizard } from './wizard';
import { ViewerInput,browserViewerKey,viewerLayers,viewerTools } from '../viewer-input.mjs';
import {viewerExportScale} from '../viewer-zoom.mjs';
import {initialViewerView} from '../viewer-geometry.mjs';
import { AutoUpdate,pageNeedsUpdate,advancedControlsMatch,advancedFields,wizardPageSettings } from '../auto-update.mjs';
import { PaletteSelection } from '../palette-selection.mjs';
import { suggestProfileName } from '../profile-name.mjs';
import { request } from './api.mjs';
import { BatchWorkspace } from './batch';
import {completedResultPage,resultBackPage} from '../review-state.mjs';
import {activeProcessingSteps,completedProcessingSteps} from '../processing-job.mjs';

const textEntry = target => target instanceof HTMLElement &&
  (target.matches('input, textarea, select, [role="slider"]') || target.isContentEditable);

function Logo() {
  return <svg viewBox="0 0 32 32" width="28" height="28" aria-hidden="true"><path d="M5 25 15.5 5H21L10.5 25ZM18 25l5.5-10.5L29 25Z" fill="currentColor" /></svg>;
}

function OverwriteConfirmation({confirmation,onFinish}) {
  const initialFocus = useRef(null);
  const finish = accepted => onFinish(confirmation.id,accepted);
  const saving=['directory','file'].includes(confirmation.stage);
  const flatten=confirmation.stage==='flatten';
  const edits=confirmation.stage==='palette'?'palette':'segmentation';
  let title=`Overwrite ${edits==='palette'?'Palette':'Segmentation'} Edits?`,
    description=`Replace your ${edits} edits with a new result?`,action='Overwrite '+edits;
  if(confirmation.stage==='directory') {
    title='Create directory?';description='The save directory does not exist. Create it?';action='Create directory';
  } else if(confirmation.stage==='file') {
    title='Overwrite existing file?';description='Replace this existing file?';action='Overwrite file';
  } else if(confirmation.stage==='profile') {
    title='Overwrite Settings';description=`Overwrite the saved settings “${confirmation.name}”?`;action='Overwrite settings';
  } else if(flatten) {
    title='Will Flatten Image';description='Your result may contain transparency but the selected export format does not support it. Choose PNG for transparent bitmap output.';action='OK';
  }
  return <Dialog open onOpenChange={open=>{if(!open) finish(flatten);}}>
    <DialogContent showCloseButton={false} onInteractOutside={event=>event.preventDefault()}
      onOpenAutoFocus={event=>{event.preventDefault();initialFocus.current?.focus();}}
      onCloseAutoFocus={event=>{
        event.preventDefault();
        if (confirmation.opener?.isConnected) confirmation.opener.focus();
      }}>
      <DialogHeader><DialogTitle>{title}</DialogTitle><DialogDescription>{description}</DialogDescription></DialogHeader>
      {saving && <p className="save-target">{confirmation.filename}</p>}
      <div className="confirmation-actions">{!flatten && <Button ref={initialFocus} variant="outline" onClick={()=>finish(false)}>Cancel</Button>}
        <Button ref={flatten?initialFocus:undefined} onClick={()=>finish(true)}>{action}</Button></div>
    </DialogContent>
  </Dialog>;
}

export function App() {
  const [workspace,setWorkspace] = useState({mode:'loading',batch:null,error:''});
  // Auto-update belongs to the window. Switching workspace views must not
  // recreate its checked choices or turn them back on.
  const automatic=useRef(null);
  const [profileNames,setProfileNames] = useState([]);
  const [profileName,setProfileName] = useState('');
  const [sourceHistory,setSourceHistory] = useState(null);
  const [skipReview,setSkipReview] = useState(false);
  const [quickSave,setQuickSave] = useState(null);
  const [bitmapSave,setBitmapSave] = useState(null);
  const applyQuickSave = next => setQuickSave(previous=>
    previous && ['basic','advanced'].every(mode=>['format','directory','expanded','filename'].every(key=>previous[mode][key]===next[mode][key]))?previous:next);
  const [savedFiles,setSavedFiles]=useState({});
  const savedDeadline=Math.min(...Object.values(savedFiles).map(saved=>saved.deadline));
  useEffect(()=>{
    if(!Number.isFinite(savedDeadline))return;
    const timer=setTimeout(()=>setSavedFiles(previous=>Object.fromEntries(Object.entries(previous).filter(([,saved])=>saved.deadline>performance.now()))),Math.max(0,savedDeadline-performance.now()));
    return ()=>clearTimeout(timer);
  },[savedDeadline]);
  const resetProfileName = names => setProfileName(suggestProfileName(names));
  useEffect(() => {
    const controller = new AbortController();
    Promise.all(['batch','profiles','sources','autoUpdatePreferences','reviewPreferences','quickSave','bitmapSave'].map(action=>request(action,undefined,{signal:controller.signal}))).then(([batch,profiles,sources,autoUpdate,review,quickSave,bitmapSave])=>{
      automatic.current=new AutoUpdate(autoUpdate);
      setSkipReview(review.skipReview);
      setQuickSave(quickSave);
      setBitmapSave(bitmapSave);
      setProfileNames(profiles.names);resetProfileName(profiles.names);setSourceHistory(sources);setWorkspace({mode:batch.active || batch.job.running ? 'batch' : 'single',batch,error:''});
    }).catch(error => {if (error.name !== 'AbortError') setWorkspace({mode:'loading',batch:null,error:error.message});});
    return () => controller.abort();
  },[]);
  if (workspace.mode === 'loading') return <main className="empty-view" role={workspace.error ? 'alert' : 'status'}>
    <p>{workspace.error || 'Opening workspace'}</p>{workspace.error && <Button onClick={() => window.location.reload()}>Reload</Button>}</main>;
  if (workspace.mode === 'batch') return <BatchWorkspace brand={<Logo />} initial={workspace.batch}
    profileNames={profileNames} onProfileNames={setProfileNames}
    onSingle={batch => setWorkspace({mode:'single',batch,error:''})} />;
  return <ImageWorkspace profileNames={profileNames} onProfileNames={setProfileNames}
    bitmapSave={bitmapSave} onBitmapSaveSnapshot={setBitmapSave}
    sourceHistory={sourceHistory} onSourceHistory={setSourceHistory} initialExportOptions={workspace.batch.exportOptions} automatic={automatic}
    quickSave={quickSave} onQuickSaveSnapshot={applyQuickSave} onQuickSaveChange={async input=>{
      const next=await request('quickSave',input);applyQuickSave(next);return next;
    }}
    savedFiles={savedFiles} onQuickSaved={(kind,mode,filename)=>setSavedFiles(previous=>({...previous,[kind+':'+mode]:{filename,deadline:performance.now()+5000}}))}
    skipReview={skipReview} onSkipReviewChange={async value=>{
      const next=await request('reviewPreferences',{skipReview:value});setSkipReview(next.skipReview);
    }}
    profileName={profileName} onProfileName={setProfileName} onResetProfileName={()=>resetProfileName(profileNames)} onBatch={async () => {
    const batch = await request('batch',{command:'enter'});setWorkspace({mode:'batch',batch,error:''});
  }} />;
}

function ImageWorkspace({onBatch,profileNames,onProfileNames,profileName,onProfileName,onResetProfileName,sourceHistory,onSourceHistory,initialExportOptions,automatic,skipReview,onSkipReviewChange,quickSave,onQuickSaveSnapshot,onQuickSaveChange,savedFiles,onQuickSaved,bitmapSave,onBitmapSaveSnapshot}) {
  const viewportView=useRef(null);
  if(viewportView.current===null)viewportView.current=initialViewerView();
  const [document, renderDocument] = useState(null);
  const [settings, renderSettings] = useState(null);
  const [page, renderPage] = useState('load');
  const current = useRef({document:null,settings:null,page:'load'});
  const paletteOwner = useRef(null);
  if (paletteOwner.current === null) paletteOwner.current = new PaletteSelection();
  const [paletteState,renderPalette] = useState(()=>paletteOwner.current.snapshot());
  const [autoState,renderAuto] = useState(()=>automatic.current.snapshot());
  const [confirmations,renderConfirmations] = useState([]);
  // Event handlers can run before React renders the previous pop. Keep one
  // synchronous owner so only the actual top warning can resolve a decision.
  const confirmationStack = useRef([]),nextConfirmation = useRef(0);
  const processingQueue = useRef(Promise.resolve()),automaticDue = useRef(null);
  const setSettings = next => {
    current.current.settings = typeof next === 'function' ? next(current.current.settings) : next;
    renderSettings(current.current.settings);
  };
  const setDocument = next => {
    if(next) {
      const {quickSave,...image}=next;
      if(quickSave)onQuickSaveSnapshot(quickSave);
      if(image.bitmapSave)onBitmapSaveSnapshot(image.bitmapSave);
      current.current.document={...image,bitmap:image.bitmap ?? current.current.document?.bitmap};
    } else current.current.document=null;
    renderDocument(current.current.document);
  };
  const setPage = next => {
    const previous = current.current.page;
    current.current.page = next;renderPage(next);
    const {settings,document} = current.current;
    if (settings && document) {
      if (previous === 'palette' && next !== previous) queueSelection('leavePalette',null);
      const selected = wizardPageSettings(next,settings);
      if (selected !== settings) {setSettings(selected);queueSelection('select',{settings:selected});}
      automatic.current.enterPage(selected.advanced ? next : null,selected,document.stageKeys,performance.now());
      renderAuto(automatic.current.snapshot());
    }
  };
  const syncPalette = () => {
    const selected = paletteOwner.current.snapshot();renderPalette(selected);
    setSettings(previous=>({...previous,palette:selected.palette}));
  };
  const [status, setStatus] = useState({ kind: 'idle', message: '' });
  const [processing,setProcessing] = useState(null);
  const cancelProcessing = useCallback(async () => {
    try {setProcessing(await request('processing',{command:'cancel'}));}
    catch (error) {setStatus(previous => ({...previous,message:error.message}));}
  },[]);
  useEffect(() => {
    if(status.kind!=='busy') {setProcessing(null);return;}
    const controller=new AbortController();let active=true,timer;
    const poll=async()=>{
      try {const value=await request('processing',undefined,{signal:controller.signal});if(active)setProcessing(value);}
      catch(error) {if(active && error.name!=='AbortError')setStatus(previous=>({...previous,message:error.message}));}
      finally {if(active)timer=setTimeout(poll,100);}
    };
    void poll();
    return ()=>{active=false;clearTimeout(timer);controller.abort();};
  },[status.kind]);
  useEffect(()=>{
    if(!processing?.running)return;
    const keydown=event=>{if(event.key==='Escape' && !event.repeat){event.preventDefault();void cancelProcessing();}};
    window.addEventListener('keydown',keydown);
    return ()=>window.removeEventListener('keydown',keydown);
  },[processing?.running,cancelProcessing]);
  const [dropping, setDropping] = useState(false);
  const viewerInput = useRef(null);
  if (viewerInput.current === null) viewerInput.current = new ViewerInput();
  // The view owner handles ordered input immediately. React renders its owned
  // snapshots; queued document gestures preserve the same tool transitions.
  const [viewerState,setViewerState] = useState(() => viewerInput.current.snapshot());
  const layer = viewerLayers[viewerState.primaryLayer];
  const shownLayer = viewerLayers[viewerState.currentLayer];
  const editTool = Object.keys(viewerTools).find(tool => viewerTools[tool] === viewerState.tool);
  const setLayer = layer => setViewerState(viewerInput.current.setLayer(viewerLayers.indexOf(layer),
    {segmentsEnabled:Boolean(current.current.document?.segments),vectorEnabled:Boolean(current.current.document?.preview)}));
  const setEditTool = tool => setViewerState(viewerInput.current.setTool(viewerTools[tool]));
  const [editColor, setEditColor] = useState([1,1,1,1]);
  const gestureQueue = useRef([]);
  const gestureRunning = useRef(false);
  const [gesturePending, setGesturePending] = useState(false);
  const [editOrigin, setEditOrigin] = useState('review');
  const [outputOptions, renderOutputOptions] = useState(initialExportOptions);
  const exportOptions={...outputOptions,format:quickSave[settings?.advanced?'advanced':'basic'].format};
  // Gesture requests serialize in their own queue and keep accepting samples.
  const interactionBlocked = status.kind === 'busy' || confirmations.length > 0;
  const busy = interactionBlocked || gesturePending;
  const progressLabel=processing?.current && (processing.current.complete ? completedProcessingSteps : activeProcessingSteps)[processing.current.stage];
  let displayedStatus = status;
  if (confirmations.length && status.kind === 'idle') displayedStatus={kind:'busy',message:'Waiting for confirmation…'};
  else if (processing?.running && progressLabel) {
    const message=processing.status==='cancelling' ? 'Cancelling…' : `${progressLabel}: ${processing.current.progress}%`;
    displayedStatus={kind:'busy',message};
  }
  const advancedChanged = Boolean(settings?.advanced && document &&
    !advancedControlsMatch(settings,paletteState.count,document.committedSettings,document.committedCount));
  const dirty = settings?.advanced && ['palette','segmentation','smoothing'].includes(page) &&
    ((advancedChanged && (page !== 'palette' || settings.paletteMode === 1)) || pageNeedsUpdate(page,settings,document.stageKeys));
  const historyChanged = page === 'editing' && document.editor.key !== document.stageKeys.segmentationEdits;
  const revert = dirty || historyChanged;
  const set = (key, value) => {
    if (current.current.settings[key] === value) return;
    setSettings(previous=>({...previous,[key]:value}));
    queueSelection('select',{settings:current.current.settings});
    if (current.current.settings.advanced) {
      automatic.current.settingsChanged(performance.now());renderAuto(automatic.current.snapshot());
    }
  };
  const applyDocument = setDocument;
  const applySelection = next => {
    if (current.current.document) setDocument({...current.current.document,...next});
  };
  const restorePalette = next => {
    paletteOwner.current = new PaletteSelection({count:next.count,palette:next.settings.palette,
      paletteColorCount:next.paletteColorCount,runtime:next.paletteRuntime});
    renderPalette(paletteOwner.current.snapshot());
  };
  // Render manual edits immediately, then send the same commands in order.
  // Later processing and profile operations wait for this application queue.
  const queueSelection = (action,input) => {
    const pending = processingQueue.current.then(async () => applySelection(await request(action,input)));
    processingQueue.current = pending.catch(error => {
      if (Object.hasOwn(error,'state')) applyDocument(error.state);
      setStatus({kind:'error',message:error.message});
    });
  };
  const perform = (message, operation) => {
    const pending = processingQueue.current.then(async()=>{
      setStatus({kind:'busy',message});
      try {const value=await operation();setStatus({kind:'idle',message:''});return value;}
      catch(error) {
        if (Object.hasOwn(error,'state')) applyDocument(error.state);
        if (error.state?.editor) {setEditTool(error.state.editor.tool);setEditColor(error.state.editor.color);}
        setStatus({kind:error.code==='PROCESSING_CANCELLED'?'cancelled':'error',message:error.code==='PROCESSING_CANCELLED'?'Cancelled':error.message});
      }
    });
    processingQueue.current = pending;return pending;
  };
  const setExportOptions = patch => {
    if(Object.hasOwn(patch,'format'))return perform('Updating export format…',()=>
      onQuickSaveChange({mode:current.current.settings.advanced?'advanced':'basic',format:patch.format}));
    return perform('Updating export settings…',async()=>{
      const selected=await request('outputOptions',patch);
      renderOutputOptions(selected);
    });
  };

  // A timer wakes the existing state owner. Settings and page events alone
  // change its deadlines; stale document keys after Cancel cannot reschedule it.
  const deadline = automatic.current.nextDeadline();
  useEffect(()=>{
    if (deadline === null) return;
    let timer;
    const wake = () => {
      const next = automatic.current.nextDeadline();
      if (next === null) return;
      const remaining = next-performance.now();
      if (remaining > 0) {timer=setTimeout(wake,Math.ceil(remaining));return;}
      const due = automatic.current.advance(performance.now());renderAuto(automatic.current.snapshot());
      for (const stage of due) automaticDue.current(stage);
    };
    timer=setTimeout(wake,Math.max(0,Math.ceil(deadline-performance.now())));
    return ()=>clearTimeout(timer);
  },[deadline]);

  // Preserve every dispatched point while a worker request is running. Tool
  // and color selections display immediately; synchronize once the queue drains.
  const enqueueGesture = events => {
    const operation = page === 'background' ? 'background' : 'edit';
    gestureQueue.current.push(...events);
    if (gestureRunning.current) return;
    gestureRunning.current = true;setGesturePending(true);
    if (events.some(event => ['press','select','color'].includes(event.phase))) setStatus({kind:'idle',message:''});
    const drain = async () => {
      try {
        while (gestureQueue.current.length) {
          const batch = gestureQueue.current.splice(0);
          const next = await request(operation,{action:'gesture',events:batch});
          applyDocument(next);
          if (!gestureQueue.current.length) {
            setEditTool(operation === 'background' ? next.background.tool : next.editor.tool);
            if (operation === 'edit') setEditColor(next.editor.color);
          }
        }
      } catch (error) {
        gestureQueue.current.length = 0;
        if (Object.hasOwn(error,'state')) {
          applyDocument(error.state);
          if (error.state?.editor) {setEditTool(error.state.editor.tool);setEditColor(error.state.editor.color);}
          else if (operation === 'background' && error.state?.background) setEditTool(error.state.background.tool);
        }
        setStatus({kind:'error',message:error.message});
      } finally {gestureRunning.current = false;setGesturePending(false);}
    };
    void drain();
  };
  const selectEditTool = tool => {
    setViewerState(viewerInput.current.selectTool(viewerTools[tool]));
    enqueueGesture([{phase:'select',tool}]);
  };
  const selectEditColor = color => {setEditColor(color);enqueueGesture([{phase:'color',color}]);};

  // Real synchronization with the local application session on initial load.
  useEffect(() => {
    const controller = new AbortController();
    request('state', undefined, { signal: controller.signal }).then(next => {
      if (next) { setDocument(next); setSettings(next.settings);
        restorePalette(next);
        setLayer(next.editor ? 'segments' : next.preview ? 'vector' : 'bitmap');
        setPage(next.editor ? 'editing' : next.background.active ? 'background' : next.svg ? completedResultPage(next.settings,skipReview) : next.preview ? 'smoothing' : 'mode');
        if (next.background.active) setEditTool(next.background.tool);
        if (next.editor) {
          setEditOrigin(next.settings.advanced ? 'segmentation' : 'review');setEditTool(next.editor.tool);setEditColor(next.editor.color);
        } }
    }).catch(error => { if (error.name !== 'AbortError') setStatus({ kind: 'error', message: error.message }); });
    return () => controller.abort();
  }, []);
  useEffect(() => { window.document.getElementById('wizard-title')?.focus({ preventScroll: true }); }, [page]);

  const adoptSource = next => {
    setDocument(next);setSettings(next.settings);restorePalette(next);onResetProfileName();
    setLayer('bitmap');setEditTool('pan');setPage('mode');
  };
  const openSource = (command={command:'pick'}) => perform(command.command==='reveal-directory'?'Opening folder…':'Opening image…',async()=>{
    try {
      const next=await request('sources',{...command,settings:current.current.settings??undefined,count:paletteOwner.current.snapshot().count});
      onSourceHistory(next.history);
      if(next.document)adoptSource(next.document);
    } catch(error) {
      // The host distinguishes a failed reopen from a successful load whose
      // history could not be saved. Only the latter starts a new wizard.
      if(error.sourceResult) {
        onSourceHistory(error.sourceResult.history);
        if(error.sourceResult.loaded)adoptSource(error.state);
      }
      throw error;
    }
  });
  const openFile = file => {
    if (!file || busy) return;
    perform('Opening image…', async () => {
      const bytes = await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result.slice(reader.result.indexOf(',') + 1));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const next = await request('open', { name: file.name, bytes, settings: current.current.settings ?? undefined,count:paletteOwner.current.snapshot().count });
      adoptSource(next);
    });
  };
  const analyze = async controls => {
    const next = await request('palettes', { settings: controls, lower: 2, upper: controls.paletteMode === 0 ? 2 : 12,count:paletteOwner.current.snapshot().count });
    applyDocument(next);
    const candidate = next.palettes.reduce((best, item) => !best || item.score < best.score ? item : best, null);
    if (candidate) {
      const palette = candidate.colors.flat();paletteOwner.current.choose(palette);
      applySelection(await request('palette',{action:'choose',palette}));
    }
    renderPalette(paletteOwner.current.snapshot());
    const chosen = { ...controls, palette: paletteOwner.current.snapshot().palette };
    setSettings(chosen); return chosen;
  };
  const process = async (controls, through) => {
    const next = await request('process', { settings: controls, through,count:paletteOwner.current.snapshot().count });
    applyDocument(next); setLayer(through === 'fitting' ? 'vector' : 'segments');
    if (!next.editor) setEditTool('pan');
  };
  const controlsMatch = () => {
    const {settings,document} = current.current;
    return advancedControlsMatch(settings,paletteOwner.current.snapshot().count,document.committedSettings,document.committedCount);
  };
  const askConfirmation = input => new Promise(resolve=>{
    const confirmation = {...input,id:++nextConfirmation.current,resolve,opener:window.document.activeElement};
    confirmationStack.current = [...confirmationStack.current,confirmation];
    renderConfirmations(confirmationStack.current);
  });
  const finishConfirmation = (id,accepted) => {
    const confirmation = confirmationStack.current.at(-1);
    if (confirmation?.id !== id) return;
    confirmationStack.current = confirmationStack.current.slice(0,-1);
    renderConfirmations(confirmationStack.current);confirmation.resolve(accepted);
  };
  const replayProfile = (command,name) => perform('Loading saved settings…',async()=>{
    const next = await request('profiles',{command,name});onProfileNames(next.names);
    applyDocument(next.document);setSettings(next.selection.settings);restorePalette(next.selection);
    if (next.replay.page !== null) {
      setPage(next.replay.page);setLayer(command === 'play' ? 'bitmap' : 'vector');setEditTool('pan');
    }
  });
  const saveProfile = async name => {
    const pending = await perform('Saving settings…',()=>{
      const force = profileNames.includes(name);
      return {force,decision:force ? askConfirmation({stage:'profile',name}) : Promise.resolve(true)};
    });
    if (!pending || !await pending.decision) return;
    return perform('Saving settings…',async()=>{
      const next = await request('profiles',{command:'save',name,force:pending.force});onProfileNames(next.names);onProfileName('');
    });
  };
  const manageProfile = input => perform('Updating saved settings…',async()=>{
    const next = await request('profiles',input);onProfileNames(next.names);
  });
  const requestUpdate = async stage => {
    const message = 'Updating '+(stage === 'smoothing' ? 'vector' : stage)+'…';
    // Inspect state after prior worker operations, but release that queue while
    // a warning waits. Native modal loops still dispatch armed auto-update timers.
    const pending = await perform(message,()=>{
      const overwrite = stage === 'palette' ? paletteOwner.current.snapshot().customized
        : stage === 'segmentation' && current.current.document.segmentationHistory.undoCount > 0;
      return {decision:overwrite ? askConfirmation({stage}) : Promise.resolve(true)};
    });
    if (!pending || !await pending.decision) return;
    return perform(message,async()=>{
      // A nested accepted update can change the model before this warning resolves.
      const controls = current.current.settings;
      if (stage === 'palette') {
        const count = paletteOwner.current.snapshot().count;
        const next = await request('palettes',{settings:controls,lower:count,upper:count,count});applyDocument(next);
        const palette = next.palettes.find(candidate=>candidate.count===count).colors.flat();
        paletteOwner.current.replace(palette);syncPalette();
        applySelection(await request('palette',{action:'replace',palette}));
      } else if (stage === 'segmentation') await process(controls,'segmentation');
      else {
        const next = await request('vectors',{settings:controls,count:paletteOwner.current.snapshot().count});
        applyDocument(next);setLayer('vector');
      }
    });
  };
  automaticDue.current = requestUpdate;
  const paletteAction = (action,...args) => {
    paletteOwner.current[action](...args);syncPalette();
    const command = {action};
    if (action === 'choose') command.palette = args[0];
    if (action === 'color') {command.index = args[0];command.color = args[1];}
    if (action === 'delete') command.index = args[0];
    queueSelection('palette',command);
  };
  const changeCount = count => {
    if (paletteOwner.current.setCount(count)) {
      renderPalette(paletteOwner.current.snapshot());automatic.current.settingsChanged(performance.now());renderAuto(automatic.current.snapshot());
      queueSelection('palette',{action:'count',count});
    }
  };
  const toggleAutomatic = () => {
    automatic.current.click(current.current.page,performance.now());renderAuto(automatic.current.snapshot());
    const checked=automatic.current.checkedPreferences();
    return perform('Updating automatic settings…',()=>request('autoUpdatePreferences',checked));
  };
  const edit = (action,command) => perform('Updating segmentation…', async () => {
    const next = await request('edit',{action,command,settings:current.current.settings});
    applyDocument(next);
    if (action === 'begin') { setEditOrigin(current.current.settings.advanced ? 'segmentation' : 'review');setEditTool(next.editor.tool);setEditColor(next.editor.color);setPage('editing');setLayer('segments'); }
    else if (action === 'finish') { setPage(current.current.settings.advanced ? 'smoothing' : completedResultPage(current.current.settings,skipReview));setLayer('vector');setEditTool('pan'); }
    else if (action === 'leave') {setPage(editOrigin);setLayer(current.current.settings.advanced ? 'segments' : 'vector');setEditTool('pan');}
    else if (action === 'restore' && next.editor) {setEditTool(next.editor.tool);setEditColor(next.editor.color);}
  });
  const background = (action,destination) => perform('Updating background…',async () => {
    const next = await request('background',{action});
    applyDocument(next);setEditTool(next.background.tool);setLayer('vector');
    if (action === 'begin') setPage('background');
    else if (action === 'finish') setPage(destination ?? (current.current.settings.advanced ? 'export' : 'review'));
  });
  const reviewAction = action => perform('Updating result…',async()=>{
    const next=await request('review',{action});applyDocument(next.document);
    setSettings(next.selection.settings);restorePalette(next.selection);setPage(next.page);
    setLayer(['type','palette'].includes(next.page) ? 'bitmap' : next.page==='editing' ? 'segments' : 'vector');
    if(next.page==='editing') {setEditOrigin('review');setEditTool(next.document.editor.tool);setEditColor(next.document.editor.color);}
    else setEditTool(next.page==='background' ? next.document.background.tool : 'pan');
  });
  const advance = (controls = current.current.settings) => {
    switch (current.current.page) {
      case 'editing': return edit('finish');
      case 'background': return background('finish');
      case 'load': setPage('mode'); break;
      case 'type': setPage(document.file.hasAlpha ? 'transparency' : 'quality'); break;
      case 'transparency':
        if (controls.advanced) {setPage('palette');setLayer('bitmap');} else setPage('quality');
        break;
      case 'quality': setPage('colors'); break;
      case 'colors': return perform('Finding colors…',async()=>{
        if (controls.paletteMode === 1) { await analyze(controls); setPage('palette'); }
        else { if (controls.paletteMode === 0) controls = await analyze(controls); await process(controls, 'fitting'); setPage(completedResultPage(controls,skipReview)); }
      });
      case 'palette':
        if (controls.advanced) {
          if ((!controlsMatch() && controls.paletteMode === 1) || pageNeedsUpdate('palette',controls,current.current.document.stageKeys)) return requestUpdate('palette');
          setPage('segmentation');setLayer('segments');
        } else return perform('Vectorizing image…',async()=>{await process(controls,'fitting');setPage(completedResultPage(controls,skipReview));});
        break;
      case 'segmentation':
        if (!controlsMatch() || pageNeedsUpdate('segmentation',controls,current.current.document.stageKeys)) return requestUpdate('segmentation');
        return edit('begin');
      case 'smoothing': {
        if (!controlsMatch() || pageNeedsUpdate('smoothing',controls,current.current.document.stageKeys)) return requestUpdate('smoothing');
        return background('begin');
      } break;
      case 'review': setPage('export');break;
      default: break;
    }
  };
  const select = (key, value) => {
    if (key === 'mode') {
      const controls = { ...settings, advanced: value === 'advanced' };
      setSettings(controls);
      if (value === 'auto') perform('Vectorizing image…', async () => {
        const next = await request('automatic', { settings: controls,count:paletteOwner.current.snapshot().count });
        applyDocument(next); setSettings(next.settings);
        restorePalette(next);
        setLayer('vector');setEditTool('pan'); setPage(completedResultPage(next.settings,skipReview));
      });
      else if (value === 'basic') {queueSelection('select',{settings:controls});setPage('type');}
      else {
        if (document.file.hasAlpha && controls.backgroundMode === -1) {controls.backgroundMode=0;controls.background=[1,1,1,0];}
        const page = document.file.hasAlpha ? 'transparency' : 'palette';
        const selected = wizardPageSettings(page,controls);
        setSettings(selected);queueSelection('select',{settings:selected});setPage(page);setLayer('bitmap');
      }
      return;
    }
    const controls = { ...settings, [key]: value };
    if (key === 'backgroundMode' && value === 1) controls.background = [1, 1, 1, 1];
    setSettings(controls);queueSelection('select',{settings:controls});advance(controls);
  };
  const back = () => {
    if (page === 'background') {background('finish',settings.advanced ? 'smoothing' : 'review');return;}
    if (page === 'export' || page === 'review') {
      const destination=resultBackPage(page,settings,skipReview);
      if(destination==='background')background('begin');
      else {setPage(destination);setLayer(['palette','colors'].includes(destination) ? 'bitmap' : 'vector');}
      return;
    }
    if (revert) return perform('Reverting changes…',async()=>{
      const {document,settings} = current.current;
      const restored={...settings};for (const name of advancedFields) restored[name]=document.committedSettings[name];
      paletteOwner.current.setCount(document.committedCount);renderPalette(paletteOwner.current.snapshot());setSettings(restored);
      await request('palette',{action:'count',count:document.committedCount});
      await request('select',{settings:restored});
      const next = await request('edit',{action:'restore'});applyDocument(next);
      if (next.editor) {setEditTool(next.editor.tool);setEditColor(next.editor.color);}
      if (restored.advanced) {automatic.current.settingsChanged(performance.now());renderAuto(automatic.current.snapshot());}
    });
    if (page === 'editing') {edit('leave');return;}
    if (page === 'smoothing' && settings.advanced) {edit('begin');return;}
    const pages = settings.advanced ? ['mode', ...(document.file.hasAlpha ? ['transparency'] : []), 'palette', 'segmentation', 'smoothing', 'export']
      : ['mode', 'type', ...(document.file.hasAlpha ? ['transparency'] : []), 'quality', 'colors', ...(settings.paletteMode === 1 ? ['palette'] : []), 'review', 'export'];
    setPage(pages[Math.max(0, pages.indexOf(page) - 1)]);
  };
  const start = () => {
    if (page !== 'load') {if (page === 'background') background('finish','load');else setPage('load');return;}
    perform('Closing image…', async () => { await request('reset', null); setDocument(null); setSettings(null);onResetProfileName(); });
  };
  const save = () => window.location.assign(`/api/export?${new URLSearchParams(exportOptions)}`);
  const bitmapCommand=(action,input)=>{
    const pending=processingQueue.current.then(async()=>{
      const mode=current.current.settings.advanced?'advanced':'basic';
      const next=await request(action,{mode,...input});
      if(action==='bitmapSave')setDocument({...current.current.document,bitmapSave:next});
      else setDocument({...current.current.document,bitmapSize:next});
      return next[mode];
    });
    const settled=pending.catch(error=>{
      if(Object.hasOwn(error,'state'))applyDocument(error.state);
      setStatus({kind:'error',message:error.message});
      return action==='bitmapSize'?current.current.document.bitmapSize[current.current.settings.advanced?'advanced':'basic']:undefined;
    });
    processingQueue.current=settled;return action==='bitmapSave'?pending:settled;
  };
  const downloadBitmap=event=>{
    const opener=event.currentTarget;
    return perform('Preparing bitmap…',async()=>{
    const mode=current.current.settings.advanced?'advanced':'basic';
    const query=new URLSearchParams({format:current.current.document.bitmapSave[mode].format});
    let response=await fetch('/api/export?'+query);
    if(!response.ok) {
      const failure=await response.json();
      if(failure.code!=='BITMAP_FLATTEN_REQUIRED')throw Object.assign(new Error(failure.error),{state:failure.state});
      await askConfirmation({stage:'flatten'});
      query.set('flatten','true');response=await fetch('/api/export?'+query);
      if(!response.ok){const failure=await response.json();throw Object.assign(new Error(failure.error),{state:failure.state});}
    }
    const disposition=response.headers.get('Content-Disposition');
    const filename=decodeURIComponent(disposition.slice(disposition.indexOf("UTF-8''")+7));
    const url=URL.createObjectURL(await response.blob()),link=window.document.createElement('a');
    link.href=url;link.download=filename;link.click();
    // Leave the URL alive for the browser's download task, then release it.
    setTimeout(()=>URL.revokeObjectURL(url),1000);
    }).then(()=>{
      // The pending request disables its button. Restore focus after React
      // commits the idle state, including when the transparency prompt closes.
      window.requestAnimationFrame(()=>opener.focus());
    });
  };
  const changeQuickSave=patch=>{
    // Blur commits precede the next Save click without disabling that button
    // mid-click. The same application queue orders both operations.
    const pending=processingQueue.current.then(async()=>{
      const mode=current.current.settings.advanced?'advanced':'basic';
      const next=await onQuickSaveChange({mode,...patch});return next[mode];
    });
    processingQueue.current=pending.catch(error=>setStatus({kind:'error',message:error.message}));return pending;
  };
  const saveToDirectory=(kind,opener)=>{
    return perform('Saving '+kind+'…',async()=>{
      const mode=current.current.settings.advanced?'advanced':'basic';
      let outcome=await request('quickSaveFile',{command:'save',kind,mode});
      while(outcome.status==='confirm') {
        if(outcome.stage==='flatten') {
          await askConfirmation({stage:'flatten'});
          outcome=await request('quickSaveFile',{command:'acknowledge'});
        } else {
          const accepted=await askConfirmation({stage:outcome.stage==='directory'?'directory':'file',filename:outcome.filename});
          outcome=await request('quickSaveFile',{command:'decide',accepted});
        }
      }
      if(outcome.status==='saved')onQuickSaved(kind,mode,outcome.filename);
    }).then(()=>window.requestAnimationFrame(()=>{
      // A normalized filename can replace its input node during the commit.
      // Its stable id identifies the same control after the save completes.
      const control=opener.id?window.document.getElementById(opener.id):opener;
      control?.focus();
    }));
  };
  const browseQuickSave=kind=>perform('Choosing save directory…',async()=>{
    const next=await request('quickSaveDirectory',{kind,mode:current.current.settings.advanced?'advanced':'basic'});
    if(kind==='bitmap')setDocument({...current.current.document,bitmapSave:next});else onQuickSaveSnapshot(next);
  });
  const canNext = Boolean(document) && !['mode', 'export'].includes(page) &&
    (page !== 'type' || settings.category >= 0) &&
    (page !== 'palette' || settings.advanced || settings.paletteMode === 2 || settings.palette.length > 0);

  useEffect(() => {
    const viewerKey = (event,phase) => {
      if (!document || busy || textEntry(event.target)) return;
      const previous = viewerInput.current.snapshot();
      const next = viewerInput.current.key(browserViewerKey(event,phase),
        {bitmapEnabled:true,vectorEnabled:Boolean(document.preview)});
      setViewerState(next);
      if (next.tool !== previous.tool && ['editing','background'].includes(page)) {
        const tool = Object.keys(viewerTools).find(tool => viewerTools[tool] === next.tool);
        enqueueGesture([{phase:'select',tool}]);
      }
      if (['b','v'].includes(event.key.toLowerCase()) || event.key === 'Shift' && ['editing','background'].includes(page)) event.preventDefault();
    };
    const keydown = event => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'o') { event.preventDefault(); if (!busy) openSource(); return; }
      if (['editing','background'].includes(page) && !busy && (event.ctrlKey || event.metaKey) &&
          !(event.target instanceof HTMLElement && (event.target.matches('input, textarea, select') || event.target.isContentEditable))) {
        const key = event.key.toLowerCase();
        const action = key === 'z' ? event.shiftKey ? 'redo' : 'undo' : key === 'y' ? 'redo' : null;
        if (action) { event.preventDefault();if (page === 'background') background(action);else edit(action);return; }
      }
      viewerKey(event,'press');
      if (busy || event.target instanceof HTMLElement && (event.target.matches('input, textarea, select, button, summary, [role="slider"], [role="combobox"]') || event.target.isContentEditable)) return;
      if (event.altKey && event.key.toLowerCase() === 'r') { event.preventDefault(); start(); }
      else if (event.key === 'PageUp' || event.altKey && ['b', 'ArrowLeft'].includes(event.key)) { event.preventDefault(); if (!['load', 'mode'].includes(page)) back(); }
      else if (event.key === 'PageDown' || event.altKey && ['n', 'ArrowRight'].includes(event.key) || event.key === 'Enter') { event.preventDefault(); if (canNext) advance(); }
    };
    const keyup = event => viewerKey(event,'release');
    window.addEventListener('keydown', keydown);
    window.addEventListener('keyup',keyup);
    return () => {window.removeEventListener('keydown', keydown);window.removeEventListener('keyup',keyup);};
  });

  return <main className={`application ${dropping ? 'file-hover' : ''}`} aria-busy={busy}
    onDragOver={event => { if (event.dataTransfer.types.includes('Files')) { event.preventDefault(); if (!busy) setDropping(true); } }}
    onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget)) setDropping(false); }}
    onDrop={event => { event.preventDefault(); setDropping(false); if (!busy) openFile(event.dataTransfer.files[0]); }}>
    <header className="app-header">
      <div className="brand"><Logo /><span>openvector</span></div>
      <span className="header-divider" />
      <div className="current-file">{document ? <><FileImage size={14} /><span title={document.file.name}>{document.file.name}</span><span className="dimensions">{document.width} × {document.height}</span></> : <span>No image open</span>}</div>
      <div className="header-actions"><Button variant="ghost" size="sm" disabled={busy} onClick={() => openSource()}><Upload />Open</Button>
        <Button variant="ghost" size="sm" disabled={busy} onClick={() => perform('Opening batch processing…',async()=>{
          await onBatch();automatic.current.enterPage(null,current.current.settings,current.current.document?.stageKeys,performance.now());
        })}><Images />Batch</Button>
        <Button variant="outline" size="sm" disabled={busy || !document?.svg} onClick={() => {
          if(page==='background')background('finish','export');else {setPage('export');setLayer('vector');}
        }}><Download />Export</Button>
        <Dialog><DialogTrigger asChild><IconButton label="About this development preview"><Info /></IconButton></DialogTrigger>
          <DialogContent><DialogHeader><DialogTitle>OpenVector</DialogTitle><DialogDescription>Free, open-source raster to vector conversion.</DialogDescription></DialogHeader>
            <p>This development preview supports PNG, BMP, JPEG, GIF and TIFF images, automatic and guided processing, and SVG, EPS, PDF, AI, DXF and EMF exports.</p>
            <p>Pencil, Fill, Zap, color picking, background removal, edit history and batch conversion are available. Saved profiles, remaining gesture cases and native desktop packaging are still being completed. Exact Vector Magic parity has not been established.</p>
          </DialogContent></Dialog>
      </div>
    </header>
    <div className="workspace"><Viewer document={document} viewModel={viewportView} onOpen={() => openSource()} disabled={interactionBlocked} layer={layer} shownLayer={shownLayer} onLayerChange={setLayer}
      editing={page === 'editing' || page === 'background'} backgroundEditing={page === 'background'} editTool={editTool} onGesture={event => enqueueGesture([event])} />
      <Wizard {...{ page, document, settings, set, busy, dirty, revert, canNext, select, exportOptions, setExportOptions,paletteState }} status={displayedStatus}
        profileNames={profileNames} onReplayProfile={replayProfile} onSaveProfile={saveProfile} onManageProfile={manageProfile} profileName={profileName} onProfileName={onProfileName}
        automatic={autoState[page]} onAutomaticChange={toggleAutomatic} onPaletteAction={paletteAction} onCountChange={changeCount}
        onReview={reviewAction} skipReview={skipReview} onSkipReviewChange={value=>perform('Updating review settings…',()=>onSkipReviewChange(value))}
        onDoneReview={()=>setPage('export')} onImprove={()=>{setPage('review');setLayer('vector');}}
        quickSave={quickSave[settings?.advanced?'advanced':'basic']} onQuickSaveChange={changeQuickSave}
        onQuickSave={opener=>saveToDirectory('vector',opener)} onQuickSaveBrowse={()=>browseQuickSave('vector')}
        bitmapSave={bitmapSave[settings?.advanced?'advanced':'basic']} onBitmapSaveChange={input=>bitmapCommand('bitmapSave',input)}
        onBitmapSize={input=>bitmapCommand('bitmapSize',input)} onBitmapDownload={downloadBitmap}
        onBitmapViewScale={()=>bitmapCommand('bitmapSize',{command:'view-scale',value:viewerExportScale(viewportView.current)})}
        onBitmapSave={opener=>saveToDirectory('bitmap',opener)} onBitmapBrowse={()=>browseQuickSave('bitmap')}
        savedFile={savedFiles['vector:'+(settings?.advanced?'advanced':'basic')]?.filename}
        bitmapSavedFile={savedFiles['bitmap:'+(settings?.advanced?'advanced':'basic')]?.filename}
        sourceHistory={sourceHistory} onReopenSource={index=>openSource({command:'reopen',index})}
        onBrowseDirectory={index=>openSource({command:'pick-directory',index})} onRevealDirectory={index=>openSource({command:'reveal-directory',index})}
        onOpen={() => openSource()} onAnalyze={() => settings.advanced ? requestUpdate('palette') : perform('Finding palettes…', () => analyze(settings))}
        onEdit={edit} onBackground={background} editTool={editTool} onToolChange={selectEditTool} editColor={editColor} onColorChange={selectEditColor}
        onBack={back} onNext={() => advance()} onStart={start} onExport={save} />
    </div>
    {confirmations.map(confirmation=><OverwriteConfirmation key={confirmation.id} confirmation={confirmation} onFinish={finishConfirmation} />)}
    <footer className="app-footer"><span className="local-indicator" />Local processing<span className="footer-state">{busy ? displayedStatus.message : status.kind==='cancelled' ? 'Cancelled' : document ? pageTitles[page] : 'Ready'}</span>
      {processing?.running && <>
        {processing.current && <progress className="single-processing-progress" aria-label={progressLabel} value={processing.current.progress} max="100" />}
        <Button className="processing-cancel" variant="ghost" size="sm" disabled={processing.status==='cancelling'} onClick={cancelProcessing}><Square size={11} />Cancel</Button>
      </>}<span>Development preview</span></footer>
    {dropping && <div className="drop-overlay"><Upload />Drop to open image</div>}
  </main>;
}
