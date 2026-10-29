import { createRequire } from 'node:module';
import { nativePath } from './native.mjs';

const path = nativePath('openvector-node.node');
let addon;
try {
  addon = createRequire(import.meta.url)(path);
} catch (cause) {
  throw new Error(
    `OpenVector could not load ${path}: ${cause.message}. ` +
    'Use Node.js 22.19 or later and an archive built for this operating system, architecture and system libraries. ' +
    'Rebuild from the source checkout with npm run build if needed.',
    { cause },
  );
}

export const {
  CoreSession,
  ImageDocument,
  BatchRunner,
  SourceHistory,
  SourceDirectories,
  SavedAdvancedProfiles,
  RecentBasicProfiles,
  decodeImage,
  defaultDocumentSettings,
  defaultAdvancedProfile,
  defaultBatchProfile,
  batchProfileControls,
  applyAdvancedProfile,
  applyBasicProfile,
  basicProfileLabel,
  applyPaletteCommand,
  resolveParameters,
  formatFileSize,
} = addon;
export default addon;
