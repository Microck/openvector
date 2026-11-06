export function rectangle() {
  const nodes=[{x:0,y:0},{x:4,y:0},{x:4,y:3},{x:0,y:3}],contour=[0,1,2,3];
  const edges=contour.map((node,i)=>[i,0,-1,node,contour[(i+1)%4],i,(i+1)%4]);
  return {width:4,height:3,geometry:{nodes,cubics:[],cubicStorageSlots:0,
    regions:[{bgra:[0,0,255,255],parent:-1,contour,edges}]},palette:[0,0,1,1]};
}
