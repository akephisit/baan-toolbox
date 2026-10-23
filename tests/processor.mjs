import{chromium}from'playwright';import assert from'node:assert/strict';
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:['--no-sandbox']});
try{
 const p=await browser.newPage();await p.goto(process.env.TEST_URL||'http://localhost:8080');
 const result=await p.evaluate(async()=>{
  const worker=new Worker('/processor-worker.js',{type:'module'});let seq=0;const pending=new Map();worker.onmessage=({data})=>{const{resolve,reject}=pending.get(data.requestId);pending.delete(data.requestId);data.ok?resolve(data.data):reject(new Error(data.error));};
  const rpc=(command,args={})=>new Promise((resolve,reject)=>{const requestId=++seq;pending.set(requestId,{resolve,reject});worker.postMessage({requestId,command,...args});});
  const job='11223344556677889900aabbccddeeff';let badOffset=false,badHash=false,tooBig=false;
  await rpc('prepare',{job,size:'3',name:'ไทย.bin'});
  try{await rpc('write',{job,offset:'1',bytes:new Uint8Array([1]).buffer});}catch{badOffset=true;}
  await rpc('write',{job,offset:'0',bytes:new Uint8Array([1,2,3]).buffer});
  try{await rpc('finish',{job,digest:'bad'});}catch{badHash=true;}
  try{await rpc('prepare',{job:'22334455667788990011aabbccddeeff',size:'9007199254740991',name:'large.bin'});}catch(e){tooBig=e.message.includes('พื้นที่');}
  const clean='33445566778899001122aabbccddeeff';await rpc('prepare',{job:clean,size:'0',name:'empty'});await rpc('abort',{job:clean});
  const source=new File([new Uint8Array([1,2,3])],'source');await rpc('register',{id:'source',file:source});
  const read=await rpc('read',{id:'source',job:'sourcehash',offset:'0',length:3});const digest=await rpc('digest',{job:'sourcehash'});
  worker.terminate();return{badOffset,badHash,tooBig,bytes:[...new Uint8Array(read.bytes)],digest:digest.digest};
 });
 assert.equal(result.badOffset,true);assert.equal(result.badHash,true);assert.equal(result.tooBig,true);assert.deepEqual(result.bytes,[1,2,3]);assert.equal(result.digest,'039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81');console.log('PASS: worker offset validation, hash mismatch cleanup, quota preflight, abort, incremental source hashing');
}finally{await browser.close();}
