/// <reference types="node" />
// Declarations for npm/index.mjs. The Node-API bindings in
// rust/crates/openvector-node convert these shapes; keep both in sync.
// Processing calls are synchronous. Progress callbacks run at Rust
// checkpoints and return true to continue or false to cancel.

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

/** Four normalized BGRA channels from 0 through 1. Palettes repeat them per color. */
export type Bgra = [number, number, number, number];

export interface DocumentSettings {
  category: number;
  quality: number;
  sensitivity: number;
  advanced: boolean;
  antiAlias: boolean;
  backgroundMode: number;
  background: Bgra;
  complexity: number;
  minimumPixels: number;
  artifactRejection: number;
  clusterColors: boolean;
  paletteMode: number;
  palette: number[];
  smoothness: number;
  detectCorners: boolean;
  curveComplexity: number;
  smoothingAntiAlias: boolean;
}

/** Selected controls plus GUI counters that stay independent of palette length. */
export interface SettingsProfile {
  settings: DocumentSettings;
  colorCount: number;
  basicColorCount: number;
  paletteColorCount: number;
  recommendedBackgroundMode: number;
}

/** A profile palette's revert copy and customization flag. */
export interface PaletteRuntime {
  backup: number[];
  customized: boolean;
}

export interface DocumentProgress {
  stage: string;
  phase: number;
  substep: number;
  counter: number;
  fraction: number;
  percent: number;
  colorCount: number | null;
  iteration: number | null;
}

export interface SnapshotSelection {
  rasters?: ('original' | 'source' | 'working')[];
  /** `regions` omits the pixel label plane. */
  segmentation?: 'regions' | 'full';
  contours?: boolean;
  smoothing?: boolean;
  fitting?: boolean;
}

export type DocumentProgressCallback = (event: DocumentProgress, checkpoint: DocumentSnapshot) => boolean;
export interface DocumentProgressOptions {
  callback: DocumentProgressCallback;
  /** Omitted means metadata only. Never call document.snapshot() in the callback. */
  capture?: SnapshotSelection;
}

export interface RasterImage {
  width: number;
  height: number;
  bgra: Buffer;
}

/** Bounded metadata, with no raster, region, geometry or editor-history arrays. */
export interface DocumentSummary {
  dimensions: { original: [number, number]; source: [number, number]; working: [number, number] };
  settings: DocumentSettings;
  classification: { category: number; quality: number; paletteMode: number; colorCount: number };
  parameters: JsonValue;
  stageKeys: {
    palette: string;
    reservedC0: string;
    segmentation: string;
    segmentationEdits: string;
    smoothing: string;
    fitting: string;
  };
  /** Indexed by palette color count, 0 through 24. */
  completed: number[];
  scores: number[];
  palettes: number[][];
  available: { segmentation: boolean; contours: boolean; smoothing: boolean; fitting: boolean };
  fittingComplete: boolean;
  processed: boolean;
  status: number;
  progress: number;
  cancelled: boolean;
  randomState: number;
}

export interface ContourGeometry {
  nodes: { x: number; y: number; kind: number; flags: number }[];
  regions: {
    /** Geometry colors are byte channels, not normalized palette channels. */
    pixelCount: number; colorIndex: number; bgra: [number, number, number, number]; parent: number;
    contour: number[]; boundary: [number, number, number][]; neighbors: number[];
  }[];
}

export interface FittedGeometry {
  cubicStorageSlots: number;
  cubics: { index: number; points: [number, number, number, number, number, number, number, number] }[];
  nodes: { x: number; y: number; kind: number; cubic: number }[];
  regions: {
    /** Geometry colors are byte channels, not normalized palette channels. */
    pixelCount: number; colorIndex: number; bgra: [number, number, number, number]; parent: number; contour: number[];
    edges: [number, number, number, number, number, number, number][];
  }[];
}

/** Unrequested bulk sections are absent; requested unavailable sections are null. */
export interface DocumentSnapshot extends DocumentSummary {
  original?: RasterImage | null;
  source?: RasterImage | null;
  working?: RasterImage | null;
  segmentation?: {
    width: number;
    height: number;
    /** Absent with `regions`; null when the checkpoint has no label plane. */
    indices?: Int32Array | null;
    palette: Float32Array;
    counts: Int32Array;
    colorIndices: Int32Array;
  } | null;
  contours?: ContourGeometry | null;
  smoothing?: { borderCounts: number[]; referencePositions: Float32Array; penaltyCounts: Uint8Array } | null;
  fitting?: {
    complete: boolean;
    geometry: FittedGeometry;
    cubicAvailability: Uint8Array;
    outputChains: { children: number[][]; holes: [number, number][][][] } | null;
  } | null;
}

export interface TaskResult {
  didWork: boolean;
  settings: DocumentSettings;
  state: DocumentSummary;
}

/**
 * The fitting task's own completion, read from the Rust stage observer.
 * Pass both fields to `RecentBasicProfiles.recordCompletion`.
 */
export interface FittingCompletion {
  status: number;
  didWork: boolean;
}

/** Returned by `runAutomatic` and `runBatchImage`. */
export interface AutomaticResult {
  settings: DocumentSettings;
  paletteCount: number | null;
  sourceAlphaType: number;
  recommendedBackgroundMode: number;
  state: DocumentSummary;
  fitting: FittingCompletion;
}

/** Compact metadata plus the fitting task's own completion. */
export interface FinishEditingResult extends DocumentSummary {
  fittingCompletion: FittingCompletion;
}

/** Shared by advanced and recent-basic profile replay. */
export interface ProfileResult {
  profile: SettingsProfile;
  paletteRuntime: PaletteRuntime;
  state: DocumentSummary;
  paletteRequested: boolean;
  /** Whether each stage did work. `palette` is present only when requested. */
  didWork: { palette?: boolean; segmentation: boolean; smoothing: boolean; fitting: boolean };
}

export interface BatchSettings {
  advanced?: boolean;
  category?: number;
  quality?: number;
  paletteMode?: number;
  colorCount?: number;
  backgroundMode?: number;
  palette?: number[];
  sensitivity?: number;
  antiAlias?: boolean;
  complexity?: number;
  minimumPixels?: number;
  artifactRejection?: number;
  clusterColors?: boolean;
  smoothness?: number;
  detectCorners?: boolean;
  curveComplexity?: number;
  smoothingAntiAlias?: boolean;
}

export interface ExportOptions {
  layout?: 'stacked' | 'cutouts' | 'grouped-cutouts';
  thinStroke?: boolean;
  compact?: boolean;
  curveMode?: 'splines' | 'sparse-lines' | 'dense-lines';
}

export interface SaveVectorResult {
  status: number;
  error: { message: string; osError: number | null } | null;
}

export interface ToolRecord {
  kind: number;
  x: number;
  y: number;
  coordinates: [number, number, number, number];
  argb: [number, number, number, number];
  key: string;
}

export interface EditorSummary {
  width: number;
  height: number;
  key: string;
  tool: string;
  color: Bgra;
  undoCount: number;
  redoCount: number;
}

export interface EditorSnapshot extends EditorSummary {
  bgra: Buffer;
  undo: ToolRecord[];
  redo: ToolRecord[];
}

export type EditCommand =
  | { tool: 'fill'; x: number; y: number; color: Bgra; commit?: boolean }
  | { tool: 'zap'; x: number; y: number; color?: Bgra | null; commit?: boolean }
  | { tool: 'pencil'; from: [number, number]; to: [number, number]; color: Bgra; commit?: boolean };

export interface PointerGesture {
  phase: 'press' | 'move' | 'release';
  point: [number, number];
  buttons: number;
  button?: number;
  modifiers?: number;
}

export type EditorGesture =
  | { phase: 'select'; tool: 'pan' | 'pencil' | 'fill' | 'zap' | 'picker' }
  | { phase: 'color'; color: Bgra }
  | PointerGesture;

export type BackgroundGesture = { phase: 'select'; tool: 'pan' | 'background' } | PointerGesture;

export interface BackgroundSummary {
  count: number;
  active: boolean;
  tool: string;
  hiddenCount: number;
  undoCount: number;
  redoCount: number;
}

export interface BackgroundSnapshot extends BackgroundSummary {
  hiddenRegions: number[];
  undo: number[];
  redo: number[];
}

export interface BatchRecord {
  filename: string;
  selected: boolean;
  outputFilename: string;
  shouldProcess: boolean;
  buttonIndex: number;
}

export interface BatchQueueSnapshot {
  exportDirectory: string;
  exportFormat: string;
  processingIndex: number;
  acceptingEdits: boolean;
  queue: BatchRecord[];
}

export interface BatchFile {
  filename: string;
  outputFilename: string;
  status: string;
  error?: string;
}

export interface BatchResult {
  status: string;
  files: BatchFile[];
  queue: BatchQueueSnapshot;
  log: string;
  converted: number;
  failed: number;
  skipped: number;
}

export interface BatchEvent {
  index: number;
  filename: string;
  stage: string;
  paletteCount: number | null;
  state: DocumentSummary;
  progress: Omit<DocumentProgress, 'stage'> | null;
}

export interface BatchProgressOptions {
  callback: (event: BatchEvent, checkpoint: DocumentSnapshot) => boolean;
  capture?: SnapshotSelection;
}

export type OverwriteDecision = 'overwrite' | 'skip' | 'cancel';

export interface SourceHistorySnapshot {
  capacity: number;
  files: { filename: string; description: string; thumbnailDimensions: [number, number] }[];
}

export interface SourceDirectoriesSnapshot {
  capacity: number;
  directories: string[];
}

export interface SavedAdvancedProfile {
  name: string;
  profile: SettingsProfile;
}

export interface RecentBasicProfile {
  label: string;
  profile: SettingsProfile;
}

export type PaletteCommand =
  | { action: 'count'; count: number }
  | { action: 'replace' | 'choose'; palette: number[] }
  | { action: 'color'; index: number; color: Bgra }
  | { action: 'delete'; index: number }
  | { action: 'add' | 'revert' | 'source' };

export interface DecodedImage {
  width: number;
  height: number;
  bgra: Buffer;
  hasAlpha: boolean;
  format: string;
}

export declare class CoreSession {
  constructor();
  randomState(): number;
  createDocument(): ImageDocument;
  createBatch(applicationDirectory: string, workingDirectory: string): BatchRunner;
}

/** Create documents with `CoreSession.createDocument()`. */
export declare class ImageDocument {
  private constructor();
  setSource(encoded: Buffer, filename: string): void;
  /** Reduce a source above 1.6 megapixels using native block averaging.
   * Returns true when resized and clears derived processing/editing state;
   * sourceAlphaType keeps the full-size source's classification, as native does.
   * Returns false without changing state when no reduction is needed. */
  shrinkSource(): boolean;
  commitSettings(settings: DocumentSettings): void;
  /** No selection explicitly copies all bulk data. `{}` captures only metadata. */
  snapshot(selection?: SnapshotSelection): DocumentSnapshot;
  /** Palette tasks need `lower` and `upper`; classification needs `extension`. */
  runTask(stage: 'classification' | 'palette' | 'segmentation' | 'smoothing' | 'fitting',
    settings: DocumentSettings & { lower?: number; upper?: number; extension?: string },
    progress?: DocumentProgressOptions): TaskResult;
  runAutomatic(controls: Partial<DocumentSettings> & { extension: string },
    progress?: DocumentProgressOptions): AutomaticResult;
  runBatchImage(controls: BatchSettings & { extension: string },
    progress?: DocumentProgressOptions): AutomaticResult;
  /** Returns null when the profile's palette mode does not process. */
  runAdvancedProfile(profile: SettingsProfile, runtime: PaletteRuntime,
    progress?: DocumentProgressOptions): ProfileResult | null;
  /**
   * Replays a recent basic profile with `advanced` false. Two-color profiles
   * search 2 colors and fixed-count profiles exactly `paletteColorCount`,
   * without the advanced 2 through 12 fallback for zero. Returns null when
   * the palette mode does not process.
   */
  runBasicProfile(profile: SettingsProfile, runtime: PaletteRuntime,
    progress?: DocumentProgressOptions): ProfileResult | null;
  beginEditing(): EditorSummary;
  /** Explicit owned pixels and full undo/redo records; null before beginEditing. */
  editing(): EditorSnapshot | null;
  /** `[undoCount, redoCount]`. */
  editingHistory(): [number, number];
  leaveEditing(): void;
  restoreCommittedEditing(): EditorSummary | null;
  edit(command: EditCommand): EditorSummary;
  gesture(event: EditorGesture): EditorSummary;
  undoEdit(): EditorSummary;
  redoEdit(): EditorSummary;
  revertEdits(): EditorSummary;
  commitEditing(): DocumentSummary;
  finishEditing(settings: DocumentSettings, progress?: DocumentProgressOptions): FinishEditingResult;
  beginBackgroundEditing(): BackgroundSummary;
  /** Explicit owned hidden-region list and full background history. */
  background(): BackgroundSnapshot;
  toggleBackground(x: number, y: number): BackgroundSummary;
  backgroundGesture(event: BackgroundGesture): BackgroundSummary;
  undoBackground(): BackgroundSummary;
  redoBackground(): BackgroundSummary;
  revertBackground(): BackgroundSummary;
  finishBackgroundEditing(): BackgroundSummary;
  /** Errors carry a `code`, such as `BITMAP_FLATTEN_REQUIRED`. */
  exportBitmap(format: string, scale?: number, flatten?: boolean): Buffer;
  /** Writes atomically only if the destination does not exist. Cancellation leaves no final file. */
  saveVector(filename: string, options: ExportOptions, progress?: (percent: number) => boolean): SaveVectorResult;
  reset(): void;
}

/** Create batch runners with `CoreSession.createBatch()`. */
export declare class BatchRunner {
  private constructor();
  /** Returns skipped duplicate filenames, not the files added to the queue. */
  addFiles(filenames: string[]): string[];
  select(index: number, selected: boolean): void;
  remove(index: number): void;
  removeSelected(): void;
  clear(): void;
  setExport(directory: string, format: string): void;
  snapshot(): { running: boolean; queue: BatchQueueSnapshot; document: DocumentSummary | null };
  /** Omitting `overwrite` skips existing files. */
  run(settings: BatchSettings, exportOptions: ExportOptions,
    overwrite?: (filenames: string[]) => OverwriteDecision,
    progress?: BatchProgressOptions): BatchResult;
}

export declare class SourceHistory {
  constructor(snapshot?: SourceHistorySnapshot);
  add(filename: string, width: number, height: number, bytes: number): SourceHistorySnapshot;
  snapshot(): SourceHistorySnapshot;
  remove(filename: string): SourceHistorySnapshot;
  clear(): SourceHistorySnapshot;
}

export declare class SourceDirectories {
  constructor(snapshot?: SourceDirectoriesSnapshot);
  add(directory: string): SourceDirectoriesSnapshot;
  snapshot(): SourceDirectoriesSnapshot;
  clear(): SourceDirectoriesSnapshot;
}

export declare class SavedAdvancedProfiles {
  constructor(entries?: SavedAdvancedProfile[]);
  snapshot(): SavedAdvancedProfile[];
  put(name: string, profile: SettingsProfile): void;
  /** Moves an exact name to the front. Index -1 and a null profile mean no match. */
  select(name: string): { index: number; profile: SettingsProfile | null };
  /** Returns the removed index, or -1. */
  remove(name: string): number;
  rename(name: string, newName: string, force: boolean): { index: number; changed: boolean; replacedIndex: number };
  reset(): void;
  load(filename: string): void;
  save(filename: string): void;
}

/** Recent Basic Settings. Rust owns labels, eligibility, duplicates and capacity. */
export declare class RecentBasicProfiles {
  /** Rebuilds a `storedProfiles()` array. Omitted means empty. */
  constructor(storedProfiles?: SettingsProfile[]);
  /** Up to five entries, newest first, with complete in-memory profiles. */
  entries(): RecentBasicProfile[];
  /** The first four entries, which the GUI shows. */
  visibleEntries(): RecentBasicProfile[];
  /**
   * Offers one fitting completion. Rust records it only for status 0, real work
   * and a non-batch run, and returns whether it was eligible. A duplicate label
   * moves the older entry to the front and keeps its complete profile.
   * `status` must be an unsigned 32-bit integer. Completions come from
   * `AutomaticResult.fitting`, `FinishEditingResult.fittingCompletion`,
   * `runTask('fitting', …)`, or a replay's `state.status` and `didWork.fitting`.
   */
  recordCompletion(profile: SettingsProfile, status: number, didWork: boolean, batch: boolean): boolean;
  clear(): void;
  /** Newest first, with palette colors omitted and opaque backgrounds. */
  storedProfiles(): SettingsProfile[];
  /** Replaces the list from a collection file. A missing file means empty. */
  load(filename: string): void;
  /** Atomically writes `{ "recentBasicSettings": storedProfiles() }`. */
  save(filename: string): void;
}

export declare function decodeImage(encoded: Buffer): DecodedImage;
export declare function defaultDocumentSettings(): DocumentSettings;
export declare function defaultAdvancedProfile(): SettingsProfile;
export declare function defaultBatchProfile(): SettingsProfile;
export declare function batchProfileControls(profile: SettingsProfile): Required<BatchSettings>;
export declare function applyAdvancedProfile(current: SettingsProfile | null | undefined,
  selected: SettingsProfile, batch: boolean): SettingsProfile;
/** The stored profile to select for replay, with `advanced` false. */
export declare function applyBasicProfile(selected: SettingsProfile): SettingsProfile;
/** The English Recent Basic Settings label for category, quality and palette mode. */
export declare function basicProfileLabel(profile: SettingsProfile): string;
export declare function applyPaletteCommand(profile: SettingsProfile, runtime: PaletteRuntime,
  command: PaletteCommand): { profile: SettingsProfile; runtime: PaletteRuntime; changed: boolean | null };
export declare function resolveParameters(options: {
  stage: 'palette' | 'segmentation' | 'smoothing' | 'fitting' | 'classification';
  [option: string]: JsonValue;
}): JsonValue;
export declare function formatFileSize(bytes: number): string;

declare const addon: {
  CoreSession: typeof CoreSession;
  ImageDocument: typeof ImageDocument;
  BatchRunner: typeof BatchRunner;
  SourceHistory: typeof SourceHistory;
  SourceDirectories: typeof SourceDirectories;
  SavedAdvancedProfiles: typeof SavedAdvancedProfiles;
  RecentBasicProfiles: typeof RecentBasicProfiles;
  decodeImage: typeof decodeImage;
  defaultDocumentSettings: typeof defaultDocumentSettings;
  defaultAdvancedProfile: typeof defaultAdvancedProfile;
  defaultBatchProfile: typeof defaultBatchProfile;
  batchProfileControls: typeof batchProfileControls;
  applyAdvancedProfile: typeof applyAdvancedProfile;
  applyBasicProfile: typeof applyBasicProfile;
  basicProfileLabel: typeof basicProfileLabel;
  applyPaletteCommand: typeof applyPaletteCommand;
  resolveParameters: typeof resolveParameters;
  formatFileSize: typeof formatFileSize;
};
export default addon;
