import test from 'node:test';
import assert from 'node:assert/strict';
import {JianShadowKernel,jianShadowOffset,jianDrawTriangularShadow} from '../src/ck_bridge.js';
function fixture() {
 const calls={compiled:0,effectDeletes:0,shaderDeletes:0,paintDeletes:0,draws:[]};
 const fields=[['bounds',0,4],['radius',4,1],['blur',5,1],['inner',6,1],['tint',7,4]];
 const ck={RuntimeEffect:{Make(source){calls.compiled++;if(source==='fail')return null;return {
  getUniformFloatCount(){return 11;},getUniformCount(){return fields.length;},
  getUniformName(i){return fields[i][0];},getUniform(i){return{slot:fields[i][1],columns:fields[i][2],rows:1};},
  makeShader(data){calls.uniforms=Array.from(data);return{delete(){calls.shaderDeletes++;}};},delete(){calls.effectDeletes++;}
 };}},Paint:class {setAntiAlias(v){calls.aa=v;}setColor(v){calls.color=v;}setShader(v){calls.shader=v;}delete(){calls.paintDeletes++;}},Color4f:(...v)=>v,LTRBRect:(...v)=>v};
 const canvas={getTotalMatrix:()=>[1,0,0,0,1,0,0,0,1],drawRect(r){calls.draws.push(r);}};
 const kernel=new JianShadowKernel(ck);kernel.setSource('kernel');return{calls,ck,canvas,kernel};
}
const shadow=[5,7,8,-2,Math.fround(10/255),Math.fround(20/255),Math.fround(30/255),Math.fround(0.05),1];
test('typed profile keeps float RGBA through one bounded reusable effect',()=>{
 const f=fixture();for(let i=0;i<2;i++)assert(jianDrawTriangularShadow(f.ck,f.canvas,f.kernel,[10,20,80,40],[6,6,6,6],shadow,null));
 assert.equal(f.calls.compiled,1);assert.equal(f.calls.shaderDeletes,2);assert.equal(f.calls.paintDeletes,2);assert.equal(f.calls.aa,false);assert.deepEqual(f.calls.color,[1,1,1,1]);
 assert.deepEqual(f.calls.uniforms.slice(0,7),[17,29,93,65,4,8,0]);
 assert.equal(f.calls.uniforms[7],Math.fround(shadow[4]*shadow[7]));assert.equal(f.calls.uniforms[10],shadow[7]);
 f.kernel.dispose();assert.equal(f.calls.effectDeletes,1);
});
test('legacy Gaussian, nonuniform corners and collapsed triangles retain explicit dispatch',()=>{
 const f=fixture();assert.equal(jianDrawTriangularShadow(f.ck,f.canvas,f.kernel,[0,0,8,8],[2,2,2,2],[...shadow.slice(0,8),0],null),false);
 assert.equal(jianDrawTriangularShadow(f.ck,f.canvas,f.kernel,[0,0,8,8],[1,2,1,2],shadow,null),false);
 const s=shadow.slice();s[3]=-5;assert(jianDrawTriangularShadow(f.ck,f.canvas,f.kernel,[0,0,8,8],[2,2,2,2],s,null));assert.equal(f.calls.compiled,0);assert.equal(f.calls.draws.length,0);
});
test('failed compile is cached and source replacement disposes the previous program',()=>{
 const f=fixture();f.kernel.setSource('fail');for(let i=0;i<3;i++)assert.equal(f.kernel.make({}),null);assert.equal(f.calls.compiled,1);
 f.kernel.setSource('kernel');assert(f.kernel.make({bounds:[0,0,1,1],radius:[0],blur:[1],inner:[0],tint:[0,0,0,1]}));
 f.kernel.setSource('next');assert.equal(f.calls.effectDeletes,1);f.kernel.setSource('a'.repeat(32769));assert.equal(f.kernel.source,'');
});
test('canvas-space offsets survive reflected rotated nonuniform transforms and DPR',()=>{
 const basis=[2,0,0,0,2,0,0,0,1], current=[-1.7,0.6,123,0.8,1.2,456,0,0,1];
 const [x,y]=jianShadowOffset(current,basis,5,7);assert(Math.abs(current[0]*x+current[1]*y-10)<1e-6);assert(Math.abs(current[3]*x+current[4]*y-14)<1e-6);
 assert.deepEqual(jianShadowOffset(basis,basis,5,7),[5,7]);assert.deepEqual(jianShadowOffset([0,0,0,0,0,0,0,0,1],basis,5,7),[5,7]);
});
