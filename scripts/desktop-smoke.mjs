import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import http from 'node:http';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const root=process.cwd();
const fixtures=path.join(root,'.tools/fixtures');
const output=path.join(root,'.tools/outputs');fs.mkdirSync(output,{recursive:true});
const browser=await chromium.connectOverCDP('http://localhost:9226');
const page=browser.contexts()[0].pages()[0];
await page.waitForSelector('#app h1');
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const results=[];
const passed=(name,detail='')=>{results.push({name,status:'passed',detail});console.log('PASS',name,detail);};
const waitJob=async(id,timeout=150000)=>{const end=Date.now()+timeout;while(Date.now()<end){const snapshot=await invoke('snapshot');const job=snapshot.jobs.find(j=>j.id===id);if(['completed','failed','skipped','canceled'].includes(job?.status))return job;await new Promise(r=>setTimeout(r,300));}throw new Error('Job timeout '+id);};
const add=async(file)=>{await invoke('add_files',{paths:[file]});const s=await invoke('snapshot');return s.jobs.at(-1);};
const s=await invoke('snapshot');
assert.equal(s.settings.ui_language,'en');assert.equal(s.settings.target_language,'el');assert.equal(s.settings.parallel_files,1);assert.equal(s.settings.allow_paid,false);passed('Initial defaults: English UI, Greek target, one file, free only');
let settings={...s.settings,shell_enabled:false,transcription_mode:'local',target_language:'en',source_language:'auto',output_directory:output};
await invoke('save_settings',{settings});
for(const model of ['base','small']) {
    settings.local_model=model;await invoke('save_settings',{settings});
    const job=await add(path.join(fixtures,'English δείγμα.mp4'));await invoke('start_queue');
    const result=await waitJob(job.id);assert.equal(result.status,'completed',result.message);
    const subtitle=fs.readFileSync(result.output,'utf8');assert.match(subtitle,/Windows computer/i);assert.match(subtitle,/00:00:/);passed(`Whisper ${model}: local recognition, language detection and SRT saving`,path.basename(result.output));
}
const sourceSrt='1\n00:00:00,200 --> 00:00:02,000\nHello world.\n\n2\n00:00:03,000 --> 00:00:05,000\nThank you.\n';
fs.writeFileSync(path.join(fixtures,'English δείγμα.en.srt'),sourceSrt);
// All provider failures below use synthetic text and a server on this computer.
let requests=[];let attempts=0;
const server=http.createServer(async(req,res)=>{let data='';for await(const part of req)data+=part;requests.push(req.url);res.setHeader('Content-Type','application/json');
if(req.url.startsWith('/limited')){res.statusCode=429;res.setHeader('Retry-After','1');res.end('{}');return;}
if(req.url.startsWith('/unauthorized')){res.statusCode=401;res.end('{}');return;}
if(req.url.startsWith('/retry')&&attempts++===0){res.statusCode=503;res.end('{}');return;}
if(req.url.startsWith('/malformed')){res.end('{bad json');return;}
const body=JSON.parse(data||'{}');const prompt=body.messages?.[0]?.content??body.input??'';const segments=JSON.parse(prompt.split('Input segments: ')[1]??'[]');const translated=segments.map((c,i)=>({id:c.id,text:i===0?'Γεια σου κόσμε.':'Ευχαριστώ.'}));res.end(JSON.stringify({choices:[{message:{content:JSON.stringify({language:'el',segments:translated})}}]}));});
await new Promise(r=>server.listen(9238,'127.0.0.1',r));
const base={id:'subforge-smoke-key',name:'Local fixture',kind:'custom',endpoint:'http://127.0.0.1:9238/good',models:['fixture'],enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:10,account_id:'',cli_path:'',auth:'key'};
await invoke('save_key',{id:base.id,value:'synthetic-fixture-key'});
try {
settings.target_language='el';settings.translation=[{...base,id:base.id+'-limited',name:'Limited',endpoint:'http://127.0.0.1:9238/limited'},base];await invoke('save_key',{id:base.id+'-limited',value:'synthetic-fixture-key'});await invoke('save_settings',{settings});
let job=await add(path.join(fixtures,'English δείγμα.mp4'));await invoke('start_queue');let result=await waitJob(job.id);assert.equal(result.status,'completed',result.message);assert.deepEqual(requests.slice(-2),['/limited/chat/completions','/good/chat/completions']);const translated=fs.readFileSync(result.output,'utf8');assert.match(translated,/Γεια σου κόσμε/);assert.match(translated,/00:00:00,200 --> 00:00:02,000/);passed('External subtitle translation and ordered 429 fallback preserve timing');
settings.translation=[{...base,endpoint:'http://127.0.0.1:9238/unauthorized'}];await invoke('save_settings',{settings});job=await add(path.join(fixtures,'English δείγμα.mp4'));await invoke('start_queue');result=await waitJob(job.id);assert.equal(result.status,'failed');assert.match(result.message,/http_401/);passed('Invalid credentials produce a recoverable error');
settings.translation=[{...base,endpoint:'http://127.0.0.1:9238/good'}];await invoke('save_settings',{settings});await invoke('job_action',{id:job.id,action:'retry'});result=await waitJob(job.id);assert.equal(result.status,'completed',result.message);passed('Retry uses updated provider settings and cached source');
settings.translation=[{...base,endpoint:'http://127.0.0.1:9238/retry',retries:1}];await invoke('save_settings',{settings});job=await add(path.join(fixtures,'English δείγμα.mp4'));await invoke('start_queue');result=await waitJob(job.id);assert.equal(result.status,'completed',result.message);assert.equal(attempts,2);passed('Transient 503 retries before provider fallback');
settings.translation=[{...base,id:base.id+'-malformed',endpoint:'http://127.0.0.1:9238/malformed'},base];await invoke('save_key',{id:base.id+'-malformed',value:'synthetic-fixture-key'});await invoke('save_settings',{settings});job=await add(path.join(fixtures,'English δείγμα.mp4'));await invoke('start_queue');result=await waitJob(job.id);assert.equal(result.status,'completed',result.message);passed('Malformed JSON falls back without corrupting cue IDs');
settings.translation=[{...base,cost:'paid'}];settings.allow_paid=false;await invoke('save_settings',{settings});const before=requests.length;job=await add(path.join(fixtures,'English δείγμα.mp4'));await invoke('start_queue');result=await waitJob(job.id);assert.equal(result.status,'failed');assert.equal(requests.length,before);passed('Paid route blocked without issuing a request');
const exported=path.join(output,'settings-export.json');await invoke('export_settings',{path:exported});assert(!fs.readFileSync(exported,'utf8').includes('synthetic-fixture-key'));passed('Settings export excludes secrets');
await page.locator('[data-page="settings"]').click();await page.locator('[data-field="theme"]').selectOption('light');assert.equal(await page.locator('html').getAttribute('data-theme'),'light');passed('Light theme');
for(const lang of ['en','el','es','fr','de','pt','it','zh-Hans','ko','ja']){await page.locator('[data-field="ui_language"]').selectOption(lang);assert.equal(await page.locator('html').getAttribute('lang'),lang);assert(await page.locator('h1').innerText());}passed('All ten interface languages render');
await page.locator('[data-field="ui_language"]').selectOption('en');await page.locator('[data-field="theme"]').selectOption('dark');await page.locator('[data-page="help"]').click();for(const lang of ['en','el','es','fr','de','pt','it','zh-Hans','ko','ja']){await page.locator('#help-language').selectOption(lang);await page.waitForFunction(lang=>document.querySelector('iframe')?.contentDocument?.documentElement?.lang===lang,lang);const sections=await page.frameLocator('iframe').locator('section').count();assert.equal(sections,8);}passed('Ten offline help guides load inside desktop app');
await page.locator('[data-page="queue"]').click();await page.screenshot({path:path.join(root,'artifacts/SubForge-desktop.png'),fullPage:true});
} finally {await invoke('save_key',{id:base.id,value:''});await invoke('save_key',{id:base.id+'-limited',value:''});await invoke('save_key',{id:base.id+'-malformed',value:''});await new Promise(r=>server.close(r));fs.writeFileSync(path.join(root,'artifacts/desktop-tests.json'),JSON.stringify(results,null,2));await browser.close();}
