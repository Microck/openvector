import { documentSegmentationKey,documentSmoothingKey,documentFittingKey } from '../src/document-controls.mjs';

export const autoUpdatePages = {palette:12,segmentation:13,smoothing:16};
export const defaultAutoUpdatePreferences = Object.freeze({palette:true,segmentation:true,smoothing:true});

/** Validate the complete checked projection at HTTP and storage boundaries. */
export function readAutoUpdatePreferences(input) {
  const pages=Object.keys(autoUpdatePages);
  if(!input || typeof input!=='object' || Array.isArray(input) || Object.keys(input).length!==pages.length ||
      !pages.every(page=>Object.hasOwn(input,page) && typeof input[page]==='boolean'))
    throw new Error('Auto-update preferences must contain palette, segmentation and smoothing Boolean choices.');
  return Object.fromEntries(pages.map(page=>[page,input[page]]));
}
export const advancedFields = ['sensitivity','antiAlias','complexity','minimumPixels','artifactRejection',
  'clusterColors','smoothness','detectCorners','curveComplexity','smoothingAntiAlias'];

/** The native advanced palette view normalizes two-color mode on entry,
 * after transparency and before its Auto-Update callback observes settings. */
export function wizardPageSettings(page,settings) {
  return page === 'palette' && settings.advanced && settings.paletteMode === 0
    ? {...settings,paletteMode:1} : settings;
}

export function advancedControlsMatch(settings,count,committed,committedCount) {
  return count === committedCount && advancedFields.every(name=>settings[name]===committed[name]);
}

/** Native page-entry and Update-button predicates use cache keys, even when
 * an earlier edit left a matching key with unavailable vector geometry. The
 * caller supplies validated controls and the document's owned key snapshot. */
export function pageNeedsUpdate(page,settings,stageKeys) {
  if (page === 'palette') return settings.paletteMode !== 2 && settings.palette.length === 0;
  if (page === 'segmentation') return documentSegmentationKey(settings) !== stageKeys.segmentation;
  if (page === 'smoothing') return documentSmoothingKey(settings,stageKeys.segmentationEdits) !== stageKeys.smoothing ||
    documentFittingKey(settings,stageKeys.segmentationEdits) !== stageKeys.fitting;
  return false;
}

/** Three CustomTimerCheckBox equivalents, with one-shot 500 ms deadlines.
 * Time is supplied by the host's monotonic clock, so native recorded events
 * and real browser timers use the same owner without sleeping in tests. */
export class AutoUpdate {
  #page = null;
  #widgets;

  constructor(checked=defaultAutoUpdatePreferences) {
    // Checked values arrive from the validated startup boundary. No timer or
    // pending work survives a process restart, even when a box was unchecked.
    this.#widgets=Object.fromEntries(Object.keys(autoUpdatePages).map(page =>
      [page,{checked:checked[page],enabled:false,pending:false,deadline:null}]));
  }

  checkedPreferences() {
    return Object.fromEntries(Object.entries(this.#widgets).map(([page,widget])=>[page,widget.checked]));
  }

  snapshot() {
    return Object.fromEntries(Object.entries(this.#widgets).map(([page,widget]) =>
      [page,{checked:widget.checked,enabled:widget.enabled,pending:widget.pending,active:widget.deadline !== null}]));
  }

  nextDeadline() {
    const active = Object.values(this.#widgets).flatMap(widget => widget.deadline === null ? [] : [widget.deadline]);
    return active.length ? Math.min(...active) : null;
  }

  #trigger(widget,now) {
    if (!widget.enabled) return;
    widget.pending = true;
    if (widget.checked) widget.deadline = now+500;
  }

  settingsChanged(now) {
    for (const widget of Object.values(this.#widgets)) this.#trigger(widget,now);
  }

  enterPage(page,settings,stageKeys,now) {
    if (page === this.#page) return;
    this.#page = page;
    for (const [name,widget] of Object.entries(this.#widgets)) {
      const enabling = name === page && !widget.enabled;
      widget.enabled = name === page;
      // Reentering the native page restarts checked pending work even when
      // its palette is valid or selected controls match the cached vector.
      if (enabling && widget.pending && widget.checked) this.#trigger(widget,now);
    }
    if (Object.hasOwn(this.#widgets,page) && pageNeedsUpdate(page,settings,stageKeys))
      this.#trigger(this.#widgets[page],now);
  }

  click(page,now) {
    const widget = this.#widgets[page];
    if (!widget.enabled) return;
    widget.checked = !widget.checked;
    if (widget.checked && widget.pending) this.#trigger(widget,now);
  }

  /** Expiry clears the running timer even if the box is unchecked/disabled.
   * Only an enabled checked expiry clears pending and emits an update. This
   * happens before processing or an overwrite dialog can succeed or fail. */
  advance(now) {
    const due = [];
    for (const [page,widget] of Object.entries(this.#widgets)) {
      if (widget.deadline === null || widget.deadline > now) continue;
      widget.deadline = null;
      if (widget.checked && widget.enabled) {widget.pending = false;due.push(page);}
    }
    return due;
  }
}
