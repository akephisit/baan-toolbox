import{chromium}from'playwright';import assert from'node:assert/strict';
const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH||undefined,args:['--no-sandbox']});const errors=[];
async function device(name){const c=await browser.newContext();await c.addInitScript(name=>{localStorage.setItem('toolbox.name',name);Object.defineProperty(window,'showSaveFilePicker',{value:undefined});Object.defineProperty(window,'showDirectoryPicker',{value:undefined});},name);const p=await c.newPage();p.on('pageerror',e=>errors.push(e.message));await p.goto(process.env.TEST_URL||'http://localhost:8080');await p.getByText('พร้อมเชื่อมต่อ',{exact:true}).waitFor({timeout:30000});return p;}
try{
 const sender=await device('Queue sender');const receivers=[];for(let i=0;i<4;i++)receivers.push(await device(`Queue receiver ${i}`));
 await sender.locator('#file-input').setInputFiles({name:'queued.txt',mimeType:'text/plain',buffer:Buffer.from('queued transfer')});
 for(let i=0;i<4;i++){const button=sender.getByRole('button',{name:new RegExp(`Queue receiver ${i}`)});await button.waitFor();await button.click();}
 await sender.getByRole('button',{name:/ส่งให้ 4 คน/}).click();await sender.getByText('รอคิว',{exact:true}).waitFor();
 const first=await Promise.race(receivers.map((p,i)=>p.getByRole('button',{name:'รับไฟล์',exact:true}).waitFor({timeout:60000}).then(()=>i)));
 await receivers[first].getByRole('button',{name:'ปฏิเสธ',exact:true}).click();
 const accepted=receivers.filter((_,i)=>i!==first);for(const p of accepted){await p.getByRole('button',{name:'รับไฟล์',exact:true}).waitFor({timeout:60000});await p.getByRole('button',{name:'รับไฟล์',exact:true}).click();}
 for(const p of accepted)await p.locator('.download-row').waitFor({timeout:60000});
 await sender.waitForFunction(()=>[...document.querySelectorAll('.transfer-top p')].filter(e=>e.textContent.includes('ส่งสำเร็จ')).length===3,{},{timeout:30000});assert.equal(await sender.getByText('ผู้รับปฏิเสธไฟล์',{exact:true}).count(),1);assert.deepEqual(errors,[]);console.log('PASS: four selected receivers, three active slots, queued recipient starts after rejection, other recipients finish');
}catch(e){console.error(e);process.exitCode=1;}finally{await browser.close();}
