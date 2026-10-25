// Run against the isolated processing-regression debug build.
import fs from 'node:fs';import path from 'node:path';import net from 'node:net';import http from 'node:http';import assert from 'node:assert/strict';import {spawn,execFileSync} from 'node:child_process';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE||'playwright');
const root=process.cwd(),exe=path.join(root,'src-tauri/target/debug/subforge.exe');
assert(fs.readFileSync(exe).includes(Buffer.from('gr.sidos.subforge.processing-regression')));
const data=path.join(root,'.tools/translation-repair-'+Date.now());fs.mkdirSync(path.join(data,'outputs'),{recursive:true});
let mode='repair',browser,page,child;const calls=[],checks=[];const pass=name=>{checks.push(name);console.log('PASS',name);};
const server=http.createServer(async(req,res)=>{
 let raw='';for await(const part of req)raw+=part;const body=JSON.parse(raw),input=body.input;
 const segments=JSON.parse(input.match(/Input segments: (\[[\s\S]*\])$/)[1]);const repair=input.startsWith('CORRECTION REQUIRED');
 calls.push({mode,provider:req.url,ids:segments.map(c=>c.id),repair,schema:!!body.text});
 if(mode==='cooldown'&&calls.filter(c=>c.mode===mode&&c.ids[0]===segments[0].id).length===1){await new Promise(r=>setTimeout(r,(segments[0].id-1)*800));res.writeHead(429,{'Retry-After':'2'});res.end('{}');return;}
 if(mode==='legacy'&&body.text){res.writeHead(400);res.end('{}');return;}
 const translated=segments.map(c=>({id:c.id,text:((mode==='repair'&&!repair)||mode==='forever')&&req.url==='/primary/responses'&&c.id===2?'Ελληνικά كل':c.id===3?'♪ Δοκιμή 3. ♪':`Δοκιμή ${c.id}.`}));
 const result={segments:translated};if(mode!=='missing-label')result.language='el';
 res.setHeader('Content-Type','application/json');res.end(JSON.stringify({output:[{content:[{text:JSON.stringify(result)}]}]}));
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;
const listener=net.createServer();await new Promise(r=>listener.listen(0,'127.0.0.1',r));const cdp=listener.address().port;await new Promise(r=>listener.close(r));
const provider=name=>({id:`repair-fixture-${name}-${Date.now()}`,name,kind:'responses',endpoint:`http://127.0.0.1:${port}/${name}`,models:['fixture'],enabled:true,cost:'free',credit_enabled:false,retries:name==='primary'?2:0,delay_ms:0,timeout_secs:30,account_id:'',cli_path:'',auth:'key'});
const providers=[provider('primary'),provider('secondary')];
const settings={ui_language:'en',target_language:'el',source_language:'en',transcription_mode:'local',parallel_files:1,parallel_batches:1,batch_size:8,speech:[],translation:providers,shell_enabled:false,output_directory:path.join(data,'outputs')};
fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify(settings));
const base=path.join(data,'base.mp4');execFileSync('src-tauri/resources/bin/ffmpeg.exe',['-hide_banner','-loglevel','error','-f','lavfi','-i','color=c=black:s=160x90:r=1','-t','30','-c:v','mpeg4','-y',base],{windowsHide:true});
const wait=async(fn)=>{const until=Date.now()+60000;while(Date.now()<until){const value=await fn();if(value)return value;await new Promise(r=>setTimeout(r,100));}throw Error('Timed out');};
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
async function run(name){mode=name;const from=calls.length,file=path.join(data,`${name}.mp4`);fs.copyFileSync(base,file);fs.writeFileSync(file.replace('.mp4','.en.srt'),Array.from({length:4},(_,i)=>`${i+1}\n00:00:${String(i*5).padStart(2,'0')},000 --> 00:00:${String(i*5+3).padStart(2,'0')},000\nSynthetic dialogue ${i+1}.\n`).join('\n'));await invoke('add_files',{paths:[file]});const id=(await invoke('snapshot')).jobs.at(-1).id;await invoke('start_queue');const job=await wait(async()=>{const j=(await invoke('snapshot')).jobs.find(j=>j.id===id);return ['completed','failed'].includes(j.status)?j:false;});assert.equal(job.status,'completed',job.message);const text=fs.readFileSync(job.output,'utf8');assert.equal((text.match(/-->/g)||[]).length,4);assert(!/[♪♫♬♩\p{Script=Arabic}]/u.test(text));for(let i=1;i<=4;i++)assert(text.includes(`Δοκιμή ${i}.`));return calls.slice(from);}
try{
 child=spawn(exe,[],{env:{...process.env,SUBFORGE_DATA_DIR:data,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${cdp}`},windowsHide:true,stdio:'ignore'});
 await wait(async()=>{try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${cdp}`);return true;}catch{return false;}});page=browser.contexts()[0].pages()[0];await page.waitForSelector('#app h1');for(const p of providers)await invoke('save_key',{id:p.id,value:'synthetic-repair-fixture-key'});
 const repaired=await run('repair');assert.deepEqual(repaired.map(c=>c.ids),[[1,2,3,4],[2]]);assert(repaired[0].schema&&repaired[1].schema&&repaired[1].repair);pass('Only the foreign-script cue is retried; valid lines, lyrics, IDs and timing survive');
 const forever=await run('forever');assert.deepEqual(forever.filter(c=>c.provider==='/primary/responses').map(c=>c.ids),[[1,2,3,4],[2],[2]]);assert(forever.some(c=>c.provider==='/secondary/responses'));pass('Exhausted repairs fall back without restarting whole-batch retries on the same provider');
 const legacy=await run('legacy');assert.equal(legacy.length,3);assert(legacy[0].schema&&legacy[1].schema&&!legacy[2].schema);pass('A gateway without structured-output support retains validated translation compatibility');
 const missing=await run('missing-label');assert.equal(missing.length,1);pass('A missing language label does not reject text whose actual target language passes validation');
 settings.batch_size=1;settings.parallel_batches=4;settings.translation[1].enabled=false;await invoke('save_settings',{settings});const cooldown=await run('cooldown');assert.equal(cooldown.length,8);pass('Concurrent parts wait through shared cooldown extensions without exhausting retries before sending requests');
 fs.writeFileSync('artifacts/translation-repair-regression.json',JSON.stringify({passed:checks.length,checks,calls},null,2));
}finally{if(page&&browser)for(const p of providers){try{await invoke('save_key',{id:p.id,value:''});}catch{}}if(browser)await browser.close();if(child&&child.exitCode===null)child.kill();server.closeAllConnections();await new Promise(r=>server.close(r));}
