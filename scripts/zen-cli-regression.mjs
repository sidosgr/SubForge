import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const root=path.resolve('.tools/zen-cli-test-data');
const browser=await chromium.connectOverCDP('http://localhost:9228');
const page=browser.contexts()[0].pages()[0];await page.waitForSelector('#app h1');
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const results=[];const pass=name=>{results.push({name,status:'passed'});console.log('PASS',name);};
const ids=['subforge-012-zen-fixture','subforge-012-custom-fixture'];
const models=['space-bunny-free','muse-spark-1.3-contributor-free','mimo-v2.6-flash-free'];
const provider={id:ids[0],name:'OpenCode Zen fixture',kind:'zen',endpoint:'http://127.0.0.1:9241/forbidden-direct',models,enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:30,account_id:'',cli_path:'never-use-this.exe',auth:'key'};
const logs=()=>fs.existsSync(path.join(root,'cli-fixture.log'))?fs.readFileSync(path.join(root,'cli-fixture.log'),'utf8').trim().split('\n').filter(Boolean).map(line=>line.trim().split('\t')):[];
const mode=value=>{fs.writeFileSync(path.join(root,'fixture-mode.txt'),value);fs.rmSync(path.join(root,'attempted'),{force:true});fs.writeFileSync(path.join(root,'cli-fixture.log'),'');};
let requests=[];
const server=http.createServer(async(req,res)=>{let body='';for await(const chunk of req)body+=chunk;requests.push(req.url);res.setHeader('Content-Type','application/json');
 if(req.url.startsWith('/fallback/chat/completions')){const cues=JSON.parse(JSON.parse(body).messages[0].content.split('Input segments: ')[1]);res.end(JSON.stringify({choices:[{message:{content:JSON.stringify({language:'el',segments:cues.map(c=>({id:c.id,text:'Δοκιμή.'}))})}}]}));}
 else {res.statusCode=500;res.end('{}');}});await new Promise(resolve=>server.listen(9241,'127.0.0.1',resolve));
const previous=(await invoke('snapshot')).settings;
const fixtureStem=path.join(root,'regression-'+Date.now());
fs.copyFileSync(path.resolve('.tools/fixtures/English δείγμα.mp4'),fixtureStem+'.mp4');
fs.copyFileSync(path.resolve('.tools/fixtures/English δείγμα.en.srt'),fixtureStem+'.en.srt');
const waitJob=async id=>{const end=Date.now()+50000;while(Date.now()<end){const job=(await invoke('snapshot')).jobs.find(j=>j.id===id);if(['completed','failed','canceled'].includes(job.status))return job;await new Promise(r=>setTimeout(r,200));}throw Error('Queue timeout');};
const queue=async translation=>{
 const settings={...previous,ui_language:'en',target_language:'el',source_language:'en',shell_enabled:false,allow_paid:false,translation,speech:[],output_directory:path.join(root,'outputs'),output_format:'srt',overwrite:false};
 await invoke('save_settings',{settings});await invoke('add_files',{paths:[fixtureStem+'.mp4']});const job=(await invoke('snapshot')).jobs.at(-1);await invoke('start_queue');return waitJob(job.id);
};
try{
 await invoke('save_key',{id:provider.id,value:'synthetic-subforge-012-key'});
 await invoke('save_key',{id:ids[1],value:'synthetic-fallback-key'});
 await invoke('save_settings',{settings:{...previous,shell_enabled:false,translation:[provider],ui_language:'en'}});
 await page.locator('[data-page="settings"]').click();await page.locator('[data-edit="translation:0"]').click();
 assert.equal(await page.locator('[data-provider-field="endpoint"]').count(),0);assert.equal(await page.locator('[data-provider-field="cli_path"]').count(),0);assert.equal(await page.locator('#agent-prepare').count(),1);assert.equal(await page.locator('#provider-key').inputValue(),'••••••••••••');pass('Existing Zen provider opens as a managed agent with its saved key visibly masked');
 mode('success');await page.locator('#provider-test').click();await page.waitForFunction(()=>document.querySelector('#provider-result')?.textContent==='Connection verified');
 let events=logs();assert.equal(events[0][1],'opencode/space-bunny-free');assert.deepEqual(events[0].slice(2,5),['true','true','true']);assert.equal(requests.length,0);pass('Connection test runs the hidden CLI with the saved key and never calls the direct Zen endpoint');
 await page.locator('#provider-models').click();await page.waitForSelector('#available-models');const catalog=await page.locator('#available-models option').evaluateAll(options=>options.map(o=>o.value));assert.deepEqual(catalog,models);pass('Model refresh uses the CLI cache refresh and returns only Zen model IDs');
 await page.screenshot({path:'artifacts/SubForge-Zen-CLI.png'});await page.locator('#modal-close').click();
 for(const model of models){mode('success');assert.equal(await invoke('test_provider',{provider:{...provider,models:[model]},role:'translation'}),'connection_ok');assert.equal(logs()[0][1],'opencode/'+model);}pass('All three user-supplied free IDs are normalized and accepted through the CLI');
 mode('success');assert.equal(await invoke('test_provider',{provider:{...provider,models:['opencode/'+models[0]]},role:'translation'}),'connection_ok');assert.equal(logs()[0][1],'opencode/'+models[0]);pass('Already-qualified CLI model IDs are preserved without a duplicate prefix');
 mode('fallback');assert.equal(await invoke('test_provider',{provider,role:'translation'}),'connection_ok');assert.deepEqual(logs().map(e=>e[1]),models.slice(0,2).map(m=>'opencode/'+m));pass('A model-specific 403 reported with exit code zero falls back to the next model in user order');
 mode('malformed');assert.equal(await invoke('test_provider',{provider,role:'translation'}),'connection_ok');assert.equal(logs().length,2);pass('Malformed CLI output triggers the next model');
 mode('success');await assert.rejects(()=>invoke('test_provider',{provider:{...provider,models:['gpt-6.1-sol']},role:'translation'}),/paid_disabled/);assert.equal(logs().length,0);pass('Free-only mode blocks a paid model before launching any CLI process');
 mode('success');const noKey={...provider,id:'subforge-012-zen-without-key'};assert.equal(await invoke('key_saved',{id:noKey.id}),false);assert.equal(await invoke('test_provider',{provider:noKey,role:'translation'}),'connection_ok');assert.equal(logs()[0][2],'false');assert.equal(logs()[0][3],'false');pass('Keyless free CLI requests do not inherit another provider credential');
 mode('retry');const retried=await queue([{...provider,models:[models[0]],retries:1,delay_ms:200}]);assert.equal(retried.status,'completed',retried.message);events=logs();assert.equal(events.length,2);assert(Number(events[1][5])-Number(events[0][5])>=4900);const text=fs.readFileSync(retried.output,'utf8');assert.match(text,/00:00:00,200 --> 00:00:02,000/);assert.match(text,/Γεια σου/);pass('Full subtitle job retries a transient CLI error, observes delay, and preserves cue timestamps');
 mode('fail-all');requests=[];const fallback=await queue([{...provider,models:[models[0]]},{...provider,id:ids[1],name:'Custom fallback',kind:'custom',models:['fixture'],endpoint:'http://127.0.0.1:9241/fallback'}]);assert.equal(fallback.status,'completed',fallback.message);assert.deepEqual(requests,['/fallback/chat/completions']);pass('Exhausted Zen CLI models move to the next configured provider without a direct Zen API fallback');
 assert(fs.readdirSync(path.join(root,'agent-tasks')).every(name=>!fs.existsSync(path.join(root,'agent-tasks',name,'prompt.txt'))));pass('Temporary subtitle request attachments are removed after both success and failure');
}finally{
 for(const id of ids)await invoke('save_key',{id,value:''});
 await invoke('save_settings',{settings:previous});
 fs.writeFileSync('artifacts/zen-cli-013-regression.json',JSON.stringify(results,null,2));
 await new Promise(resolve=>server.close(resolve));await browser.close();
}
