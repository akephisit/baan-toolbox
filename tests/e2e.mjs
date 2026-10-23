import { chromium } from 'playwright';
import { createHash, randomBytes } from 'node:crypto';
import { mkdirSync, readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const url=process.env.TEST_URL || 'http://localhost:8080';
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:['--no-sandbox']});
const errors=[]; const contexts=[];
async function device(name,mobile=false){
 const context=await browser.newContext({viewport:mobile?{width:390,height:844}:{width:1440,height:1050},acceptDownloads:true,...(mobile?{isMobile:true,hasTouch:true}:{})});contexts.push(context);
 await context.addInitScript(({name})=>{localStorage.setItem('toolbox.name',name);Object.defineProperty(window,'showSaveFilePicker',{value:undefined,configurable:true});Object.defineProperty(window,'showDirectoryPicker',{value:undefined,configurable:true});},{name});
 const p=await context.newPage();p.on('pageerror',e=>errors.push(name+': '+e.message));await p.goto(url);await p.getByText('พร้อมเชื่อมต่อ',{exact:true}).waitFor({timeout:30000});return p;
}
async function waitDone(page,count){await page.waitForFunction(count=>document.querySelectorAll('.downloads-panel .download-row').length>=count,count,{timeout:120000});}
try{
 const sender=await device('เครื่องส่ง');const a=await device('มือถือเมย์',true);const b=await device('คอมบี');
 await sender.getByRole('button',{name:/มือถือเมย์/}).waitFor();await sender.getByRole('button',{name:/คอมบี/}).waitFor();
 assert.equal(await a.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false,'mobile overflow');
 mkdirSync('test-results',{recursive:true});await sender.screenshot({path:'test-results/desktop.png',fullPage:true});await a.screenshot({path:'test-results/mobile.png',fullPage:true});
 const bytes=randomBytes(1024*1024+123); const expected=createHash('sha256').update(bytes).digest('hex');
 await sender.locator('#file-input').setInputFiles([{name:'ภาพทดสอบ-ไทย.bin',mimeType:'application/octet-stream',buffer:bytes},{name:'empty.txt',mimeType:'text/plain',buffer:Buffer.alloc(0)}]);
 await sender.getByRole('button',{name:/มือถือเมย์/}).click();await sender.getByRole('button',{name:/คอมบี/}).click();
 await sender.getByRole('button',{name:/ส่งให้ 2 คน/}).click();
 await a.getByRole('button',{name:'รับไฟล์',exact:true}).waitFor({timeout:60000});await b.getByRole('button',{name:'รับไฟล์',exact:true}).waitFor({timeout:60000});
 await a.getByRole('button',{name:'รับไฟล์',exact:true}).click();await b.getByRole('button',{name:'รับไฟล์',exact:true}).click();
 await Promise.all([waitDone(a,2),waitDone(b,2)]);
 for(const receiver of [a,b]){
  const downloadPromise=receiver.waitForEvent('download');await receiver.locator('.download-row').first().getByRole('link').click();const download=await downloadPromise;const path=await download.path();assert.equal(createHash('sha256').update(readFileSync(path)).digest('hex'),expected);assert.equal(download.suggestedFilename(),'ภาพทดสอบ-ไทย.bin');
 }
 await sender.waitForFunction(()=>[...document.querySelectorAll('.transfer-top p')].filter(e=>e.textContent.includes('ส่งสำเร็จ')).length===2,{},{timeout:30000});
 console.log('PASS: two receivers, multi-file, Thai name, empty file, downloaded bytes/SHA-256, responsive layout');
 // Rejection is independent of accepted transfers.
 await sender.getByRole('button',{name:/ส่งให้ 2 คน/}).click();await a.getByRole('button',{name:'ปฏิเสธ',exact:true}).waitFor({timeout:30000});await a.getByRole('button',{name:'ปฏิเสธ',exact:true}).click();await b.getByRole('button',{name:'รับไฟล์',exact:true}).click();await waitDone(b,4);
 await sender.getByText('ผู้รับปฏิเสธไฟล์',{exact:true}).waitFor({timeout:30000});console.log('PASS: rejecting one recipient does not stop another');
 // Explicit group fallback remains available and produces a scannable SVG.
 await a.getByRole('button',{name:'เชื่อมต่อด้วยรหัส / QR'}).click();await a.getByRole('button',{name:/สร้างกลุ่มใหม่/}).click();await a.locator('.qr svg').waitFor();const link=await a.getByRole('textbox',{name:'ลิงก์กลุ่ม — เลือกและคัดลอก'}).inputValue();
 await b.getByRole('button',{name:'เชื่อมต่อด้วยรหัส / QR'}).click();await b.getByRole('textbox',{name:'ลิงก์หรือรหัสกลุ่ม'}).fill(link);await b.getByRole('button',{name:'เข้ากลุ่ม',exact:true}).click();await b.getByRole('button',{name:/มือถือเมย์/}).waitFor({timeout:30000});assert.equal(await b.locator('.recipient').count(),1);console.log('PASS: room link/QR isolates discovery group');
 assert.deepEqual(errors,[]);console.log('PASS: no browser runtime errors');
}catch(e){console.error(e);for(let i=0;i<contexts.length;i++){for(const page of contexts[i].pages()){console.error('DEVICE',i,(await page.locator('body').innerText()).slice(-5000));await page.screenshot({path:`test-results/failure-${i}.png`,fullPage:true}).catch(()=>{});}}process.exitCode=1;}finally{await browser.close();}
