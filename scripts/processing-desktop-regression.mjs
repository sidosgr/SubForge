// Build first with TAURI_CONFIG={"identifier":"gr.sidos.subforge.processing-regression"}.
// This test uses its own data directory, local HTTP fixtures and synthetic media.
import fs from 'node:fs';import path from 'node:path';import net from 'node:net';import http from 'node:http';
import assert from 'node:assert/strict';import {spawn,execFileSync} from 'node:child_process';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE||'playwright');
const root=process.cwd(),exe=path.join(root,'src-tauri/target/debug/subforge.exe');
assert(fs.readFileSync(exe).includes(Buffer.from('gr.sidos.subforge.processing-regression')),'Use the isolated regression build, never the installed app');
const data=path.join(root,'.tools/processing-010-data-'+Date.now());fs.mkdirSync(data,{recursive:true});fs.mkdirSync(path.join(data,'outputs'));
const checks=[],calls=[],pageErrors=[];let child,browser,page,mode='parallel',active=0,maxActive=0;
const pass=name=>{checks.push(name);console.log('PASS',name);};
const wait=async(fn,timeout=60000)=>{const until=Date.now()+timeout;while(Date.now()<until){const result=await fn();if(result)return result;await new Promise(r=>setTimeout(r,100));}throw Error('Timed out');};
const server=http.createServer(async(req,res)=>{
 let raw='';for await(const part of req)raw+=part;
 const input=JSON.parse(raw).input;const segments=JSON.parse(input.match(/Input segments: (\[[\s\S]*\])$/)[1]);
 const entry={provider:req.url,ids:segments.map(c=>c.id),start:Date.now(),mode};calls.push(entry);active++;maxActive=Math.max(maxActive,active);
 let open=true;res.on('close',()=>{if(open){open=false;active--;entry.closed=Date.now();}});
 const delay=mode==='cancel'?20000:mode==='failure'&&segments[0].id===1?3500:segments[0].id===1?6000:1500;
 if(mode==='limit'&&req.url==='/primary/responses'){
  res.writeHead(429,{'Retry-After':'120'});res.end('{}');return;
 }
 await new Promise(r=>setTimeout(r,delay));if(res.destroyed)return;
 if(mode==='failure'&&segments[0].id===1){res.writeHead(400);res.end('{}');return;}
 res.setHeader('Content-Type','application/json');res.end(JSON.stringify({output:[{content:[{text:JSON.stringify({language:'el',segments:segments.map(c=>({id:c.id,text:'Δοκιμή '+c.id+'.'}))})}]}]}));entry.done=Date.now();
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;
const listener=net.createServer();await new Promise(r=>listener.listen(0,'127.0.0.1',r));const cdp=listener.address().port;await new Promise(r=>listener.close(r));
const provider=(name,url)=>({id:'processing-fixture-'+name+'-'+Date.now(),name,kind:'responses',endpoint:`http://127.0.0.1:${port}/${url}`,models:['fixture'],enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:30,account_id:'',cli_path:'',auth:'key'});
const providers=[provider('Primary','primary'),provider('Fallback','secondary')];
const settings={ui_language:'en',target_language:'el',source_language:'en',theme:'dark',transcription_mode:'local',local_model:'base',parallel_files:2,parallel_batches:2,request_interval_secs:15,batch_size:3,speech:[],translation:providers,shell_enabled:false,output_directory:path.join(data,'outputs')};
fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify(settings));
const base=path.join(data,'synthetic.mp4');execFileSync(path.join(root,'src-tauri/resources/bin/ffmpeg.exe'),['-hide_banner','-loglevel','error','-f','lavfi','-i','color=c=black:s=160x90:r=1','-t','65','-c:v','mpeg4','-q:v','8','-y',base],{windowsHide:true});
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const snapshot=()=>invoke('snapshot');const job=id=>snapshot().then(s=>s.jobs.find(j=>j.id===id));
const done=id=>wait(async()=>{const j=await job(id);return ['completed','failed','canceled'].includes(j.status)?j:false;},90000);
const stamp=seconds=>`00:${String(Math.floor(seconds/60)).padStart(2,'0')}:${String(seconds%60).padStart(2,'0')},000`;
async function add(name){const file=path.join(data,name+'.mp4');fs.copyFileSync(base,file);fs.writeFileSync(file.replace('.mp4','.en.srt'),Array.from({length:12},(_,i)=>`${i+1}\n${stamp(i*5)} --> ${stamp(i*5+3)}\nFixtureLine ${i+1}.\n`).join('\n'));await invoke('add_files',{paths:[file]});const id=(await snapshot()).jobs.at(-1).id;await invoke('start_queue');return id;}
function checkOutput(j){assert.equal(j.status,'completed',j.message);const text=fs.readFileSync(j.output,'utf8');assert.equal((text.match(/-->/g)||[]).length,12);const ids=[...text.matchAll(/Δοκιμή (\d+)\./g)].map(m=>Number(m[1]));assert.deepEqual(ids,Array.from({length:12},(_,i)=>i+1));for(let i=0;i<12;i++)assert(text.includes(`${stamp(i*5)} --> ${stamp(i*5+3)}`));}
try{
 child=spawn(exe,[],{env:{...process.env,SUBFORGE_DATA_DIR:data,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${cdp}`},windowsHide:true,stdio:'ignore'});
 await wait(async()=>{if(child.exitCode!==null)throw Error('Test app exited');try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${cdp}`);return true;}catch{return false;}});
 page=browser.contexts()[0].pages()[0];page.on('pageerror',e=>pageErrors.push(String(e)));await page.waitForSelector('#app h1');
 assert.equal((await snapshot()).settings.translation[0].id,providers[0].id,'Test data isolation');
 for(const p of providers)await invoke('save_key',{id:p.id,value:'synthetic-processing-key'});
 const id=await add('Parallel-demo');await wait(()=>calls.length>=2);await wait(async()=>((await job(id)).translation_progress.active.length===2));
 assert.equal(await page.locator('.batch-activity').count(),2);assert(await page.locator('[data-request-start]').count());await page.screenshot({path:'artifacts/SubForge-010-parallel.png'});
 const before=calls.length;await invoke('job_action',{id,action:'pause'});await new Promise(r=>setTimeout(r,6500));assert.equal(calls.length,before);pass('Pause prevents new requests while active parts finish safely');
 await invoke('job_action',{id,action:'resume'});const complete=await done(id);checkOutput(complete);assert.equal(maxActive,2);assert(calls.find(c=>c.ids[0]===4).done<calls.find(c=>c.ids[0]===1).done);pass('Two parts overlap, finish out of order, and preserve every cue ID and timestamp in final order');
 const primary=calls.filter(c=>c.provider==='/primary/responses');assert(primary[1].start-primary[0].start<1000);assert(primary[3].start-primary[2].start<1000);pass('Healthy requests start together without scheduled pauses, even with legacy settings');
 const entries=(await invoke('job_log',{id})).trim().split('\n').map(JSON.parse);assert(!entries.some(e=>e.event==='request_spacing'));assert(!entries.some(e=>e.event==='rate_limit_hit'||e.event==='cooldown_wait'));assert(entries.some(e=>e.event==='request_done'&&e.data.duration_ms>1000));pass('Healthy runs have no pacing waits and preserve per-request timing in the file log');
 mode='failure';const failedId=await add('Checkpoint-demo');const failed=await done(failedId);assert.equal(failed.status,'failed');assert(failed.translation_progress.completed>0);const saved=(await invoke('job_log',{id:failedId})).trim().split('\n').map(JSON.parse).filter(e=>e.event==='translation_saved').map(e=>e.data.part);
 mode='retry';const retryStart=calls.length;await invoke('job_action',{id:failedId,action:'retry'});checkOutput(await done(failedId));const retryCalls=calls.slice(retryStart);for(const part of saved)assert(!retryCalls.some(c=>c.ids.includes((part-1)*3+1)));pass('Retry reuses independently saved parallel parts and sends only unfinished work');
 mode='cancel';const canceledId=await add('Cancel-demo');const cancelStart=calls.length;await wait(()=>calls.length>=cancelStart+2);await invoke('job_action',{id:canceledId,action:'cancel'});await wait(()=>active===0,5000);const afterCancel=calls.length;await new Promise(r=>setTimeout(r,1500));assert.equal(calls.length,afterCancel);assert.equal((await job(canceledId)).status,'canceled');pass('Cancel aborts every active HTTP request and starts no further parts');
 mode='limit';const limitStart=calls.length;const limitedId=await add('Limit-demo');await wait(()=>calls.slice(limitStart).some(c=>c.provider==='/secondary/responses'));await page.locator(`[data-action="fileLog"][data-id="${limitedId}"]`).click();const initial=await page.locator('[data-job-log]').textContent();await wait(async()=>await page.locator('[data-job-log]').textContent()!==initial);await page.locator('#modal-close').click();checkOutput(await done(limitedId));
 const limited=calls.slice(limitStart);assert(limited.filter(c=>c.provider==='/primary/responses').length<=settings.parallel_batches);const raw=await invoke('job_log',{id:limitedId});const logs=raw.trim().split('\n').map(JSON.parse);assert(logs.some(e=>e.event==='rate_limit_hit'&&e.data.seconds>=120&&e.data.interval>=1&&e.data.interval<=2));assert(!raw.includes('synthetic-processing-key'));assert(!raw.includes('FixtureLine'));pass('A real 429 honors the server delay, increases spacing, and shares cooldown across parallel parts while falling back');pass('File log updates live and excludes credentials and dialogue');
 await page.locator('[data-page="settings"]').click();assert.equal(await page.locator('[data-field="parallel_batches"]').inputValue(),'2');assert.equal(await page.locator('[data-field="request_interval_secs"]').count(),0);for(const lang of ['en','el','es','fr','de','pt','it','zh-Hans','ko','ja']){await page.locator('[data-field="ui_language"]').selectOption(lang);assert(!(await page.locator('[data-field="parallel_batches"]').locator('..').textContent()).includes('parallel_batches'));}pass('Parallelism controls render correctly in all ten languages without obsolete pause settings');
 assert.deepEqual(pageErrors,[]);
}finally{
 if(page&&browser){for(const p of providers){try{await invoke('save_key',{id:p.id,value:''});}catch{}}}
 if(browser)await browser.close();if(child&&!child.killed){child.kill();}
 server.closeAllConnections();await new Promise(r=>server.close(r));
 fs.writeFileSync('artifacts/processing-desktop-regression.json',JSON.stringify({passed:checks.length,checks,pageErrors,calls:calls.map(({provider,ids,start,done,closed,mode})=>({provider,ids,start,done,closed,mode})),maxActive},null,2));
}
