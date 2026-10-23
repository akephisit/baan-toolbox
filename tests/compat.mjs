import{chromium,webkit}from'playwright';import assert from'node:assert/strict';import{mkdtempSync,rmSync,readFileSync}from'node:fs';import{tmpdir}from'node:os';import{join}from'node:path';import{createHash,randomBytes}from'node:crypto';
const chrome=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:['--no-sandbox']});
const profile=mkdtempSync(join(tmpdir(),'toolbox-webkit-'));const safariContext=await webkit.launchPersistentContext(profile,{headless:true,executablePath:process.env.WEBKIT_PATH||undefined,viewport:{width:390,height:844},acceptDownloads:true});const safari=safariContext.browser();
const errors=[];
async function device(browser,name){const context=browser===safari?safariContext:await browser.newContext({viewport:{width:390,height:844},acceptDownloads:true});await context.addInitScript(name=>{localStorage.setItem('toolbox.name',name);Object.defineProperty(window,'showSaveFilePicker',{value:undefined});Object.defineProperty(window,'showDirectoryPicker',{value:undefined});},name);const p=await context.newPage();p.on('pageerror',e=>errors.push(name+': '+e.message));await p.goto(process.env.TEST_URL||'http://localhost:8080');await p.getByText('พร้อมเชื่อมต่อ',{exact:true}).waitFor({timeout:30000});return p;}
try{
 const a=await device(chrome,'Chromium');const b=await device(safari,'WebKit');
 await a.getByRole('button',{name:/WebKit/}).waitFor();await b.getByRole('button',{name:/Chromium/}).waitFor();
 const left=randomBytes(300000),right=randomBytes(200000);
 await a.locator('#file-input').setInputFiles({name:'จากโครม.bin',mimeType:'application/octet-stream',buffer:left});await b.locator('#file-input').setInputFiles({name:'จากซาฟารี.bin',mimeType:'application/octet-stream',buffer:right});
 await a.getByRole('button',{name:/WebKit/}).click();await b.getByRole('button',{name:/Chromium/}).click();
 await Promise.all([a.getByRole('button',{name:/ส่งให้ 1 คน/}).click(),b.getByRole('button',{name:/ส่งให้ 1 คน/}).click()]);
 await Promise.all([a.getByRole('button',{name:'รับไฟล์',exact:true}).waitFor({timeout:60000}),b.getByRole('button',{name:'รับไฟล์',exact:true}).waitFor({timeout:60000})]);
 await a.getByRole('button',{name:'รับไฟล์',exact:true}).click();await b.getByRole('button',{name:'รับไฟล์',exact:true}).click();
 for(const [p,expected]of[[a,right],[b,left]]){
  await p.locator('.download-row').first().waitFor({timeout:120000});
  const downloadPromise=p.waitForEvent('download');await p.locator('.download-row a').first().click();const download=await downloadPromise;const bytes=readFileSync(await download.path());assert.equal(createHash('sha256').update(bytes).digest('hex'),createHash('sha256').update(expected).digest('hex'));
 }
 assert.deepEqual(errors,[]);console.log('PASS: Chromium ↔ WebKit simultaneous initial connection, bidirectional transfer, OPFS, SHA-256, mobile layout');
}catch(e){console.error(e);console.error(errors);for(const browser of[chrome,safari])for(const c of browser.contexts())for(const p of c.pages())console.error((await p.locator('body').innerText()).slice(-4000));process.exitCode=1;}finally{await Promise.all([chrome.close(),safariContext.close()]);rmSync(profile,{recursive:true,force:true});}
