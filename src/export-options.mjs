/** MainModel's shared output choices. Standalone file serializers retain
 * their own per-call defaults; application and batch export use these. */
export const defaultExportOptions=Object.freeze({layout:'grouped-cutouts',thinStroke:false,compact:false,curveMode:'dense-lines'});

export function readExportOptions(input,base=defaultExportOptions) {
  if(input===null || typeof input!=='object' || Array.isArray(input))throw new TypeError('Export options must be an object.');
  for(const key of Object.keys(input))if(!Object.hasOwn(defaultExportOptions,key))throw new Error('Unknown export option: '+key);
  const options={...base,...input};
  if(!['stacked','cutouts','grouped-cutouts'].includes(options.layout) || typeof options.thinStroke!=='boolean' ||
      typeof options.compact!=='boolean' || !['splines','sparse-lines','dense-lines'].includes(options.curveMode))
    throw new Error('Choose a valid layout, thinStroke/compact booleans and DXF curveMode.');
  return options;
}
