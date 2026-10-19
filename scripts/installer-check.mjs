import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import http from 'node:http';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.connectOverCDP('http://localhost:9227');
const page=browser.contexts()[0].pages()[0];
await page.waitForSelector('#app h1');
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const stage=process.argv[2];
const report=path.resolve('artifacts/installer-tests.json');
const results=fs.existsSync(report)?JSON.parse(fs.readFileSync(report,'utf8')):[];
const pass=name=>{results.push({name,status:'passed'});console.log('PASS',name);};
const id='subforge-isolated-installer-test-key';
let snapshot=await invoke('snapshot');
if(stage==='seed'){
  assert.equal(snapshot.settings.ui_language,'el');
  assert.equal(snapshot.settings.target_language,'de');
  assert.equal(snapshot.settings.parallel_files,2);
  pass('Installed release starts and reads retained profile settings');
  for(const file of ['bin/ffmpeg.exe','bin/ffprobe.exe','licenses/THIRD-PARTY-NOTICES.txt'])assert(fs.existsSync(path.resolve('.tools/installer-test',file)),file);
  pass('Installed package includes its media tools and license notices');
  await page.locator('[data-page="help"]').click();
  await page.waitForFunction(()=>document.querySelector('iframe')?.contentDocument?.documentElement?.lang==='el');
  assert.equal(await page.frameLocator('iframe').locator('section').count(),8);
  pass('Installed release opens bundled Greek help offline');
  snapshot.settings.translation=[{id,name:'Installer fixture',kind:'custom',endpoint:'http://127.0.0.1:9238',models:['fixture'],enabled:false,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:10,account_id:'',cli_path:'',auth:'key'}];
  await invoke('save_settings',{settings:snapshot.settings});
  await invoke('save_key',{id,value:'synthetic-installer-fixture-key'});
  assert.equal((await invoke('snapshot')).keys[id],true);
  fs.mkdirSync('.tools/installer-profile/models',{recursive:true});
  fs.writeFileSync('.tools/installer-profile/models/cleanup-marker','owned installer test fixture');
  pass('Installed app stores a test credential outside its settings JSON');
  assert(!fs.readFileSync('.tools/installer-profile/settings.json','utf8').includes('synthetic-installer-fixture-key'));
}else if(stage==='upgrade'){
  assert.equal(snapshot.settings.ui_language,'el');assert.equal(snapshot.settings.target_language,'de');assert.equal(snapshot.settings.parallel_files,2);
  assert.equal(snapshot.keys[id],true);
  assert(fs.existsSync('.tools/installer-profile/models/cleanup-marker'));
  pass('Installer upgrade preserves settings, models and saved credentials');
  const retained=structuredClone(snapshot.settings);
  let calls=0;
  const server=http.createServer(async(req,res)=>{
    let body='';for await(const chunk of req)body+=chunk;
    res.setHeader('Content-Type','application/json');
    if(++calls===1){res.statusCode=429;res.setHeader('Retry-After','1');res.end('{}');return;}
    const prompt=JSON.parse(body).messages[0].content;
    const segments=JSON.parse(prompt.split('Input segments: ')[1]);
    res.end(JSON.stringify({choices:[{message:{content:JSON.stringify({language:'Greek',segments:segments.map(c=>({id:c.id,text:'Δοκιμή υποτίτλων.'}))})}}]}));
  });
  await new Promise(resolve=>server.listen(9239,'127.0.0.1',resolve));
  try{
    const settings={...retained,target_language:'el',output_directory:path.resolve('.tools/installer-output'),translation:[{...retained.translation[0],enabled:true,endpoint:'http://127.0.0.1:9239',retries:1}]};
    fs.mkdirSync(settings.output_directory,{recursive:true});
    await invoke('save_settings',{settings});
    await invoke('add_files',{paths:[path.resolve('.tools/fixtures/English δείγμα.mp4')]});
    const job=(await invoke('snapshot')).jobs.at(-1);
    const started=Date.now();await invoke('start_queue');
    let result;
    while(Date.now()-started<30000){result=(await invoke('snapshot')).jobs.find(j=>j.id===job.id);if(['completed','failed'].includes(result.status))break;await new Promise(resolve=>setTimeout(resolve,200));}
    assert.equal(result.status,'completed',result.message);assert.equal(calls,2);assert(Date.now()-started>=950);
    assert.match(fs.readFileSync(result.output,'utf8'),/Δοκιμή υποτίτλων/);
    pass('Final installed release waits for 429 cooldown, retries once and accepts normalized Greek language');
  }finally{await invoke('save_settings',{settings:retained});await new Promise(resolve=>server.close(resolve));}
}else if(stage==='retained'){
  assert.equal(snapshot.settings.ui_language,'el');assert.equal(snapshot.settings.target_language,'de');
  assert.equal(snapshot.keys[id],true);assert(fs.existsSync('.tools/installer-profile/models/cleanup-marker'));
  pass('Default uninstall preserves the profile, model files and saved credential');
}else if(stage==='purged'){
  assert.equal(snapshot.settings.ui_language,'en');assert.equal(snapshot.settings.target_language,'el');
  snapshot.settings.shell_enabled=false;
  snapshot.settings.translation=[{id,name:'Installer fixture',kind:'custom',endpoint:'http://127.0.0.1:9238',models:['fixture'],enabled:false,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:10,account_id:'',cli_path:'',auth:'key'}];
  await invoke('save_settings',{settings:snapshot.settings});
  assert.equal((await invoke('snapshot')).keys[id],false);
  pass('Optional data-removal backend removes referenced credentials and restores defaults');
}
fs.writeFileSync(report,JSON.stringify(results,null,2));
await browser.close();
