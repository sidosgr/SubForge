import fs from 'node:fs';import path from 'node:path';import http from 'node:http';import assert from 'node:assert/strict';import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
const require=createRequire(import.meta.url);const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.connectOverCDP('http://localhost:9228');const page=browser.contexts()[0].pages()[0];await page.waitForSelector('#app h1');
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const results=[];const pass=(name,detail='')=>{results.push({name,status:'passed',detail});console.log('PASS',name);};
const secret='synthetic-subforge-011-regression-key';const id='subforge-011-fixture-gemini';const ownedKeys=[id];
const base={id,name:'Gemini fixture',kind:'gemini_custom',endpoint:'http://127.0.0.1:9240',models:['models/fixture-generation'],enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:10,account_id:'',cli_path:'invalid-manual-path.exe',auth:'key'};
let requests=[];
const server=http.createServer(async(req,res)=>{let body='';for await(const chunk of req)body+=chunk;requests.push(req.url);res.setHeader('Content-Type','application/json');
if(req.url==='/v1beta/models'){res.end(JSON.stringify({models:[{name:'models/fixture-generation',supportedGenerationMethods:['generateContent']},{name:'models/fixture-embedding',supportedGenerationMethods:['embedContent']}]}));return;}
if(req.url==='/v1beta/models/fixture-generation:generateContent'){assert.equal(req.headers['x-goog-api-key'],secret);res.end(JSON.stringify({candidates:[{content:{parts:[{text:JSON.stringify({language:'el',segments:[{id:1,text:'Δοκιμή σύνδεσης.'}]})}]}}]}));return;}
res.statusCode=404;res.end('{}');});await new Promise(resolve=>server.listen(9240,'127.0.0.1',resolve));
try{
 const initial=await invoke('snapshot');const antigravity=initial.settings.translation.find(p=>p.kind==='antigravity');assert.equal(antigravity.auth,'oauth');assert(antigravity.models.length);pass('Antigravity defaults to Google sign-in and a real model slug');
 let settings={...initial.settings,shell_enabled:false,translation:[base]};await invoke('save_settings',{settings});
 await page.locator('[data-page="settings"]').click();await page.locator('[data-edit="translation:0"]').click();
 assert.equal(await page.locator('[data-provider-field="account_id"]').count(),0);assert.equal(await page.locator('[data-provider-field="cli_path"]').count(),0);pass('Gemini editor hides Cloudflare ID and executable fields');
 await page.locator('#provider-key').fill(secret);await page.locator('#provider-form button[type="submit"]').click();await page.waitForFunction(()=>!document.querySelector('#modal').open);
 await page.locator('[data-edit="translation:0"]').click();assert.equal(await page.locator('#provider-key').inputValue(),'••••••••••••');assert.equal(await page.locator('#provider-key').getAttribute('type'),'password');assert.equal(await invoke('key_saved',{id}),true);pass('Saved credential field is visibly filled and masked');
 await page.locator('#provider-test').click();await page.waitForFunction(()=>document.querySelector('#provider-result')?.textContent==='Connection verified');assert(requests.includes('/v1beta/models/fixture-generation:generateContent'));pass('Gemini bare endpoint and models/ prefix produce a valid request');
 await page.locator('#provider-models').click();await page.waitForSelector('#available-models');assert.equal(await page.locator('#available-models option').count(),1);assert.equal(await page.locator('#available-models option').getAttribute('value'),'fixture-generation');pass('Model discovery excludes models without generateContent');
 await page.screenshot({path:'artifacts/SubForge-provider-fixed.png'});
 await page.locator('#provider-key').fill('');await page.locator('#provider-form button[type="submit"]').click();await page.waitForFunction(()=>!document.querySelector('#modal').open);assert.equal(await invoke('key_saved',{id}),true);
 await page.locator('[data-edit="translation:0"]').click();await page.locator('#provider-test').click();await page.waitForFunction(()=>document.querySelector('#provider-result')?.textContent==='Connection verified');pass('Saving a blank or unchanged masked field preserves the actual key');
 await page.locator('#provider-clear').click();await page.waitForFunction(()=>document.querySelector('#provider-key')?.value==='');assert.equal(await invoke('key_saved',{id}),false);pass('Explicit key deletion immediately empties the field and secure store');
 await page.locator('#modal-close').click();await page.locator('[data-edit="speech:0"]').click();await page.locator('[data-provider-field="kind"]').selectOption('cloudflare');await page.waitForSelector('[data-provider-field="account_id"]');assert.equal(await page.locator('[data-provider-field="cli_path"]').count(),0);pass('Cloudflare ID appears only for Cloudflare speech');await page.locator('#modal-close').click();
 for(const kind of ['antigravity','opencode','pi']){
  const provider={...base,id:'subforge-011-managed-'+kind,name:kind,kind,endpoint:'',models:kind==='antigravity'?['gemini-3.8-flash-low']:[],auth:'key'};ownedKeys.push(provider.id);
  await invoke('save_key',{id:provider.id,value:'synthetic-subforge-invalid-key'});
  const result=await invoke('prepare_agent',{provider});assert.equal(result,'agent_ready');
  const models=await invoke('provider_models',{provider});assert(models.length>0,kind+' catalog empty');pass(`${kind}: automatic verified runtime provisioning and background model discovery`,`${models.length} models`);
  if(kind==='antigravity'){
   const profileRoot=path.resolve('.tools/provider-test-data/agents/profiles/antigravity');const profileId=createHash('sha256').update(provider.id).digest('hex');const config=fs.readFileSync(path.join(profileRoot,profileId,'key/.gemini/antigravity-cli/settings.json'),'utf8');assert.equal(JSON.parse(config).modelProvider,'gemini');assert(!config.includes('synthetic-subforge-invalid-key'));pass('Antigravity key mode automatically configures its private profile without persisting keys');
   await assert.rejects(()=>invoke('test_provider',{provider,role:'translation'}),/authentication_failed/);pass('Official Antigravity reaches API authentication instead of failing on missing CLI setup');
  }
 }
 await invoke('save_key',{id,value:secret});settings=(await invoke('snapshot')).settings;settings.translation=[{...base,models:['missing-model']}];await invoke('save_settings',{settings});await page.locator('[data-edit="translation:0"]').click();await page.locator('#provider-test').click();await page.waitForFunction(()=>document.querySelector('#provider-result details'));assert.match(await page.locator('#provider-result').innerText(),/Model or endpoint not found/);pass('404 receives an actionable message with raw response inside Details');
 await page.locator('#modal-close').click();
}finally{
 for(const keyId of ownedKeys)await invoke('save_key',{id:keyId,value:''});
 fs.writeFileSync('artifacts/provider-regression.json',JSON.stringify(results,null,2));await new Promise(resolve=>server.close(resolve));await browser.close();
}
