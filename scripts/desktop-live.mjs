import {createRequire} from 'node:module';import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';
const require=createRequire(import.meta.url);const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.connectOverCDP('http://localhost:9226');const page=browser.contexts()[0].pages()[0];await page.waitForSelector('#app h1');
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const previous=(await invoke('snapshot')).settings;
const provider={id:'opencode-synthetic-free-check',name:'OpenCode free test',kind:'opencode',endpoint:'',models:['opencode/big-pickle','opencode/nemotron-3.5-lightning-free'],enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:1000,timeout_secs:120,account_id:'',cli_path:path.resolve('.tools/opencode/opencode.exe'),auth:'oauth'};
const report={provider:'Official OpenCode Windows CLI 1.19.0',models:provider.models,syntheticOnly:true};
try {
    report.connection=await invoke('test_provider',{provider,role:'translation'});assert.equal(report.connection,'connection_ok');console.log('PASS Real free OpenCode connection');
    const settings={...previous,target_language:'el',source_language:'en',output_format:'srt',output_directory:path.resolve('.tools/outputs'),translation:[provider],speech:[],allow_paid:false,shell_enabled:false};await invoke('save_settings',{settings});
    await invoke('add_files',{paths:[path.resolve('.tools/fixtures/English δείγμα.mp4')]});const job=(await invoke('snapshot')).jobs.at(-1);await invoke('start_queue');let result;const end=Date.now()+260000;
    while(Date.now()<end){result=(await invoke('snapshot')).jobs.find(j=>j.id===job.id);if(['completed','failed'].includes(result.status))break;await new Promise(r=>setTimeout(r,300));}
    assert.equal(result.status,'completed',result.message);const text=fs.readFileSync(result.output,'utf8');assert.match(text,/[Α-Ωα-ωΆΈΉΊΌΎΏάέήίόύώ]{3}/);assert.match(text,/00:00:00,200 --> 00:00:02,000/);report.pipeline='passed';report.subtitle=text;report.output=result.output;console.log('PASS Real Greek subtitle translation with original timing');
} catch(e) {report.error=String(e);console.log('LIVE CHECK:',String(e).slice(0,700));process.exitCode=1;}
finally {fs.writeFileSync('artifacts/live-opencode-test.json',JSON.stringify(report,null,2));await invoke('save_settings',{settings:previous});await browser.close();}
