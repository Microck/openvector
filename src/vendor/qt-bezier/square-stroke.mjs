/*
 * Copyright (C) 2009 Nokia Corporation and/or its subsidiary(-ies).
 * Copyright (C) 2026 OpenVector contributors.
 *
 * Modified JavaScript translation of selected Qt 4.5.3 QBezier and QStroker
 * routines. This file is licensed under LGPL-2.1. See license-lgpl.txt and
 * modifications.txt in this directory. The surrounding application is MIT.
 */
const equal=(a,b)=>a[0]===b[0] && a[1]===b[1];
const fuzzy=(a,b)=>Math.abs(a-b)<=1e-12*Math.min(Math.abs(a),Math.abs(b));
const add=(a,b)=>[a[0]+b[0],a[1]+b[1]];
const subtract=(a,b)=>[a[0]-b[0],a[1]-b[1]];
const multiply=(a,k)=>[a[0]*k,a[1]*k];
const length=a=>Math.sqrt(a[0]*a[0]+a[1]*a[1]);
const unit=a=>{const size=length(a);return size?multiply(a,1/size):[0,0];};
const normal=(a,b)=>unit([b[1]-a[1],a[0]-b[0]]);
const midpoint=(a,b)=>multiply(add(a,b),.5);

function pointAt(curve,t) {
  const u=1-t,a=u*u*u,b=u*u*(3*t),c=t*t*(3*u),d=t*t*t;
  return [a*curve[0][0]+b*curve[1][0]+c*curve[2][0]+d*curve[3][0],
    a*curve[0][1]+b*curve[1][1]+c*curve[2][1]+d*curve[3][1]];
}

function normalAt(curve,t) {
  const u=1-t,a=u*u,b=t*u,c=t*t;
  return [(curve[1][1]-curve[0][1])*a+(curve[2][1]-curve[1][1])*b+(curve[3][1]-curve[2][1])*c,
    -(curve[1][0]-curve[0][0])*a-(curve[2][0]-curve[1][0])*b-(curve[3][0]-curve[2][0])*c];
}

function split(curve) {
  const middle=midpoint(curve[1],curve[2]),left1=midpoint(curve[0],curve[1]),right2=midpoint(curve[2],curve[3]);
  const left2=midpoint(left1,middle),right1=midpoint(right2,middle),center=midpoint(left2,right1);
  return [[curve[0],left1,left2,center],[center,right1,right2,curve[3]]];
}

function acceptableOffset(original,shifted,threshold) {
  for (const t of [.25,.5,.75]) {
    const p=pointAt(original,t),q=pointAt(shifted,t),delta=subtract(p,q);
    if (Math.abs(delta[0]*delta[0]+delta[1]*delta[1]-.25)>threshold*.25) return false;
    const n=normalAt(original,t),size=Math.abs(n[0])+Math.abs(n[1]);
    if (size && Math.abs(n[0]*delta[1]-n[1]*delta[0])/size>threshold*.5) return false;
  }
  return true;
}

function shiftCurve(original,threshold) {
  const points=[original[0]],indices=[0];
  for (let i=1;i<4;i++) {
    if (!equal(original[i],original[i-1])) points.push(original[i]);
    indices.push(points.length-1);
  }
  if (points.length===1) return {status:'discard'};
  if (points.length>2) {
    const d1=subtract(points[1],points[0]),d2=subtract(points[2],points[0]);
    const d3=subtract(points.at(-1),points[0]);
    if (fuzzy(d1[0]*d2[1],d2[0]*d1[1]) &&
        (points.length===3 || fuzzy(d1[0]*d3[1],d3[0]*d1[1]))) {
      if (equal(points[0],points.at(-1))) return {status:'discard'};
      const offset=multiply(normal(points[0],points.at(-1)),.5);
      const start=add(points[0],offset),end=add(points.at(-1),offset),delta=subtract(end,start);
      return {status:'ok',curve:[start,add(start,multiply(delta,.33)),add(start,multiply(delta,.66)),end]};
    }
  }
  const [p0,p1,p2,p3]=original;
  const xs=original.map(p=>p[0]),ys=original.map(p=>p[1]);
  if (points.length===4 && Math.max(...xs)-Math.min(...xs)<.05 && Math.max(...ys)-Math.min(...ys)<.05) {
    // Preserve the upstream expression, including its asymmetric y term.
    const measure=(p0[0]-p1[0])*(p0[0]-p1[0])+(p0[1]-p1[1])*(p0[1]-p0[1])*
      (p2[0]-p3[0])*(p2[0]-p3[0])+(p2[1]-p3[1])*(p2[1]-p3[1]);
    const dot=(p0[0]-p1[0])*(p2[0]-p3[0])+(p0[1]-p1[1])*(p2[1]-p3[1]);
    if (dot<0 && dot*dot<.8*measure) return {status:'circle'};
  }
  let previous=normal(points[0],points[1]);
  const shifted=[add(points[0],multiply(previous,.5))];
  for (let i=1;i<points.length-1;i++) {
    const next=normal(points[i],points[i+1]),r=1+previous[0]*next[0]+previous[1]*next[1];
    shifted.push(add(points[i],fuzzy(r+1,1)?multiply(previous,.5):multiply(add(previous,next),.5/r)));
    previous=next;
  }
  shifted.push(add(points.at(-1),multiply(previous,.5)));
  const curve=indices.map(index=>shifted[index]);
  return {status:acceptableOffset(original,curve,threshold)?'ok':'split',curve};
}

function circleCurves(curve) {
  const [a,b,c,d]=curve;
  const first=normal(a,b),last=normal(c,d);
  if (fuzzy(length(subtract(b,a))+1,1) || fuzzy(length(subtract(d,c))+1,1)) return [];
  let middle=multiply(unit(add(subtract(a,b),subtract(d,c))),-1),sign=1;
  let angles=[first[0]*middle[0]+first[1]*middle[1],middle[0]*last[0]+middle[1]*last[1]]
    .map(cosine=>Math.acos(Math.max(-1,Math.min(1,cosine)))/Math.PI);
  if (angles[0]+angles[1]>1) {middle=multiply(middle,-1);angles=angles.map(angle=>1-angle);sign=-1;}
  const normals=[first,middle,last];
  const points=[add(a,multiply(first,.5)),add(midpoint(a,d),multiply(middle,.5)),add(d,multiply(last,.5))];
  return angles.map((angle,i)=>{
    const k=2*.5522847498*sign*.5*angle,n=normals[i],m=normals[i+1];
    return [points[i],add(points[i],[-n[1]*k,n[0]*k]),add(points[i+1],[m[1]*k,-m[0]*k]),points[i+1]];
  });
}

function offsetCurves(curve) {
  if (curve.every(p=>equal(p,curve[0]))) return [];
  let threshold=.25;
  // Match the bounded stack and float threshold used by the reference. The
  // retry relaxes geometric approximation, independently of the pixel pool.
  retry: while (true) {
    const pending=[curve],output=[];
    while (pending.length) {
      let givingUp=false;
      if (pending.length===10 || output.length===15-pending.length) {
        threshold=Math.fround(threshold*1.5);
        if (threshold<=2) continue retry;
        givingUp=true;
      }
      if (givingUp) {
        while (pending.length) {
          const shifted=shiftCurve(pending.pop(),threshold);
          if (shifted.status==='ok' || shifted.status==='split') output.push(shifted.curve);
        }
        return output;
      }
      const current=pending.pop(),shifted=shiftCurve(current,threshold);
      if (shifted.status==='ok') output.push(shifted.curve);
      else if (shifted.status==='circle' && 15-output.length>=2) output.push(...circleCurves(current));
      else if (shifted.status!=='discard') {
        const [left,right]=split(current);pending.push(right,left);
      }
    }
    return output;
  }
}

function strokeSide(points,cubic) {
  if (!cubic) {
    if (equal(points[0],points[1])) return null;
    const n=multiply(normal(...points),.5);
    const start=add(points[0],n),end=add(points[1],n);
    return {start,tangent:subtract(points[1],points[0]),commands:[['L',...end]],beforeEnd:start,end};
  }
  const curves=offsetCurves(points);
  if (!curves.length) return null;
  const first=curves[0],last=curves.at(-1);
  const tangent=subtract(points.slice(1).find(p=>!equal(p,points[0])),points[0]);
  // A repeated final control point must not erase the cap direction.
  const beforeEnd=!equal(last[2],last[3])?last[2]:!equal(last[1],last[3])?last[1]:last[0];
  return {start:first[0],tangent,commands:curves.map(curve=>['C',...curve.slice(1).flat()]),beforeEnd,end:last[3]};
}

function squareCap(from,to) {
  const previous=multiply(unit(subtract(from.end,from.beforeEnd)),.5);
  const following=multiply(unit(to.tangent),-.5);
  return [['L',...add(from.end,previous)],['L',...add(to.start,following)],['L',...to.start]];
}

/** One device-space M/L or M/C edge, as used by the application's seam pass.
 * Input geometry has already been validated and mapped by the caller. */
export function squareStroke(commands) {
  const cubic=commands[1][0]==='C',points=[commands[0].slice(1)];
  for (let i=1;i<commands[1].length;i+=2) points.push(commands[1].slice(i,i+2));
  const forward=strokeSide(points,cubic),backward=strokeSide(points.toReversed(),cubic);
  if (!forward || !backward) return [];
  // Equal endpoints close a subpath even without an explicit Z command.
  if (equal(points[0],points.at(-1))) return [
    ['M',...forward.start],...forward.commands,['Z'],['M',...backward.start],...backward.commands,['Z']];
  return [['M',...forward.start],...forward.commands,...squareCap(forward,backward),
    ...backward.commands,...squareCap(backward,forward),['Z']];
}
