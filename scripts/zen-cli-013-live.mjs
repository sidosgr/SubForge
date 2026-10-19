// Live checks use generated text and unique keyless profiles only.
import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';import{createRequire}from'node:module';
const require=createRequire(import.meta.url);const{chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.connectOverCDP('http://localhost:9228');const page=browser.contexts()[0].pages()[0];
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const root=path.resolve('.tools/zen-cli-test-data');const report={runtime:'Official OpenCode CLI 1.19.0',version:'0.1.3',syntheticOnly:true,userCredentialsUsed:false,models:[]};
const p={id:'subforge-013-official-keyless-'+Date.now(),name:'Official keyless Zen test',kind:'zen',endpoint:'',models:['space-bunny-free','muse-spark-1.3-contributor-free','mimo-v2.6-flash-free'],enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:1000,timeout_secs:120,account_id:'',cli_path:'',auth:'key'};
const previous=(await invoke('snapshot')).settings;
try{
 fs.copyFileSync(path.join(root,'official-opencode.exe'),path.join(root,'agents/runtimes/opencode/1.19.0/opencode.exe'));
 assert.equal(await invoke('key_saved',{id:p.id}),false);
 const catalog=await invoke('provider_models',{provider:p});report.catalog=p.models.map(model=>({model,available:catalog.includes(model)}));
 for(const model of p.models){const started=Date.now();try{const result=await invoke('test_provider',{provider:{...p,models:[model]},role:'translation'});report.models.push({model,result,elapsedMs:Date.now()-started});console.log(model,result);}catch(error){report.models.push({model,error:String(error),elapsedMs:Date.now()-started});console.log(model,String(error));}}
 assert(report.models.every(m=>m.result==='connection_ok'),'A live model did not translate the synthetic sample');
 const stem=path.join(root,'live-subtitles-'+Date.now());fs.copyFileSync('.tools/fixtures/English δείγμα.mp4',stem+'.mp4');fs.copyFileSync('.tools/fixtures/English δείγμα.en.srt',stem+'.en.srt');
 await invoke('save_settings',{settings:{...previous,shell_enabled:false,allow_paid:false,target_language:'el',source_language:'en',translation:[p],speech:[],output_directory:path.join(root,'outputs'),output_format:'srt',overwrite:false}});
 await invoke('add_files',{paths:[stem+'.mp4']});const job=(await invoke('snapshot')).jobs.at(-1);await invoke('start_queue');
 const end=Date.now()+150000;let result;while(Date.now()<end){result=(await invoke('snapshot')).jobs.find(j=>j.id===job.id);if(['completed','failed','canceled'].includes(result.status))break;await new Promise(r=>setTimeout(r,500));}
 report.subtitleJob={status:result.status,message:result.message};assert.equal(result.status,'completed',result.message);
 const text=fs.readFileSync(result.output,'utf8');assert.match(text,/00:00:00,200 --> 00:00:02,000/);assert.match(text,/[\u0370-\u03ff]/);assert.equal((text.match(/-->/g)||[]).length,2);report.subtitleJob.cues=2;report.subtitleJob.timestampsPreserved=true;report.subtitleJob.greekText=true;console.log('Full live subtitle job completed; Greek text and cue timestamps verified');
}finally{await invoke('save_settings',{settings:previous});fs.writeFileSync('artifacts/zen-cli-013-live.json',JSON.stringify(report,null,2));await browser.close();}
