import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.launch({channel:'msedge',headless:true});
const page=await browser.newPage({viewport:{width:1080,height:900}});
const errors=[];page.on('pageerror',error=>errors.push(String(error)));
await page.addInitScript(()=>{
 const provider={id:'openai_oauth',name:'OpenAI (ChatGPT)',kind:'openai_oauth',endpoint:'',models:[],enabled:false,cost:'plan',credit_enabled:false,retries:2,delay_ms:2000,timeout_secs:180,account_id:'',cli_path:'',auth:'oauth'};
 const settings={ui_language:'en',target_language:'el',source_language:'auto',theme:'dark',transcription_mode:'local',local_model:'base',cloud_fallback:false,parallel_files:1,parallel_batches:1,request_interval_secs:15,allow_paid:false,recursive:false,output_directory:'',filename_pattern:'{name}.{language}',output_format:'srt',overwrite:false,retain_transcript:false,audio_track:null,subtitle_override:'',subtitle_folder:'',chunk_seconds:300,batch_size:40,retention_days:7,speech:[],translation:[provider],shell_enabled:false};
 const fixture=window.fixture={settings,account:{status:'signed_out',account:'',welcome:false},calls:[],callbacks:{},events:{},checkError:false,version:'0.1.7',noInstaller:false,signinError:false};
 let callbackId=0;
 window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:callback=>{fixture.callbacks[++callbackId]=callback;return callbackId;},unregisterCallback:id=>delete fixture.callbacks[id],invoke:async(cmd,args={})=>{
  fixture.calls.push({cmd,args});
  if(cmd==='snapshot')return {settings:fixture.settings,jobs:fixture.jobs??[],models:[],keys:{},running:false,version:'0.1.7',cli:{}};
  if(cmd==='save_settings'){fixture.settings=structuredClone(args.settings);return;}
  if(cmd==='plugin:event|listen'){fixture.events[args.event]=args.handler;return 1;}
  if(cmd==='key_saved')return false;
  if(cmd==='openai_status')return fixture.account;
  if(cmd==='openai_signin'){if(fixture.signinError)throw Error('openai_browser_failed');fixture.account={status:'waiting',account:'',welcome:false};return;}
  if(cmd==='openai_cancel'){if(fixture.account.status==='waiting')fixture.account={status:'signed_out',account:'',welcome:false};return;}
  if(cmd==='openai_acknowledge'){fixture.account.welcome=false;return;}
  if(cmd==='openai_signout'){fixture.account={status:'signed_out',account:'',welcome:false};return true;}
  if(cmd==='provider_models')return ['gpt-fixture-first','gpt-fixture-second'];
  if(cmd==='test_provider')return 'connection_ok';
  if(cmd==='check_updates'){if(fixture.checkError)throw Error('update_network_error');return {current_version:'0.1.7',latest_version:fixture.version,available:fixture.version!=='0.1.7',installable:!fixture.noInstaller,notes:'<script>untrusted notes</script>',size:1024,url:'https://github.com/sidosgr/SubForge/releases'};}
  if(cmd==='install_update'){fixture.callbacks[fixture.events['update-progress']]({event:'update-progress',payload:{stage:'downloading',downloaded:500,total:1000}});throw Error('update_jobs_active');}
  return null;
 }};
});
const checks=[];const pass=name=>{checks.push(name);console.log('PASS',name);};
try{
 await page.goto('http://127.0.0.1:5173');await page.locator('[data-page="settings"]').click();
 assert.equal(await page.locator('[data-field="request_interval_secs"]').count(),0);
 await page.locator('[data-field="parallel_batches"]').selectOption('3');await page.locator('[data-action="save"]').click();
 await page.waitForFunction(()=>window.fixture.settings.parallel_batches===3);assert.equal(await page.evaluate(()=>window.fixture.settings.request_interval_secs),0);assert.equal(await page.evaluate(()=>window.fixture.settings.translation[0].delay_ms),0);
 await page.locator('[data-action="save"]').scrollIntoViewIfNeeded();await page.waitForSelector('#notice:visible');assert.match(await page.locator('#notice').textContent(),/saved/i);const box=await page.locator('#notice').boundingBox();assert(box.y>=0&&box.y+box.height<=900);
 await page.locator('[data-page="queue"]').click();await page.locator('[data-page="settings"]').click();assert.equal(await page.locator('[data-field="parallel_batches"]').inputValue(),'3');assert(await page.locator('#notice').isVisible());pass('Save clears legacy pauses and shows a visible confirmation that survives re-rendering');
 await page.locator('[data-action="dismiss"]').click();assert(!(await page.locator('#notice').isVisible()));
 for(const field of await page.locator('[data-field]').all()){const label=field.locator('..');assert.equal(await label.locator('.setting-info').count(),1);}
 const info=page.locator('[data-field="target_language"]').locator('..').locator('.setting-info');await info.hover();assert(await info.locator('.setting-tooltip').isVisible());assert.match(await info.textContent(),/native writing/);await page.mouse.move(0,0);await info.focus();assert(await info.locator('.setting-tooltip').isVisible());
 await page.locator('[data-field="ui_language"]').selectOption('el');assert.match(await page.locator('[data-field="target_language"]').locator('..').locator('.setting-tooltip').textContent(),/γλώσσα/);await page.locator('[data-field="ui_language"]').selectOption('en');pass('Every setting has help, available on hover and keyboard focus, including Greek explanations');
 await page.locator('[data-edit="translation:0"]').click();
 assert.equal(await page.locator('#openai-signin').textContent(),'Continue with ChatGPT');
 assert.equal(await page.locator('#provider-key').count(),0);assert.equal(await page.locator('[data-provider-field="endpoint"]').count(),0);assert.equal(await page.locator('#agent-signin').count(),0);
 await page.waitForFunction(()=>document.querySelector('#openai-account')?.textContent==='Signed out');pass('OpenAI OAuth editor uses account controls and hides key/endpoint inputs');
 await page.evaluate(()=>window.fixture.signinError=true);await page.locator('#openai-signin').click();await page.waitForFunction(()=>!document.querySelector('#openai-signin').disabled);assert.match(await page.locator('#openai-account').textContent(),/Sign-in failed/);pass('Failed browser launch leaves sign-in retryable');
 await page.evaluate(()=>window.fixture.signinError=false);await page.locator('#openai-signin').click();await page.waitForSelector('#openai-cancel:visible');await page.locator('#openai-cancel').click();await page.waitForFunction(()=>document.querySelector('#openai-account').textContent==='Signed out');pass('Sign-in can be cancelled without saving credentials');
 await page.locator('#openai-signin').click();await page.evaluate(()=>window.fixture.account={status:'connected',account:'fixture@example.test · fixture1',welcome:true});
 await page.waitForFunction(()=>document.querySelector('#openai-welcome').open);await page.locator('#openai-got-it').click();await page.waitForFunction(()=>!document.querySelector('#openai-welcome').open);assert.match(await page.locator('#openai-account').textContent(),/Using ChatGPT plan.*fixture@example.test/);pass('Connected account and first-sign-in plan confirmation are displayed');
 await page.locator('#provider-models').click();await page.waitForSelector('#available-models');await page.locator('#available-models').selectOption('gpt-fixture-second');await page.locator('#model-add').click();assert.equal(await page.locator('[data-provider-field="models"]').inputValue(),'gpt-fixture-second');pass('Account model discovery fills the provider model selector');
 await page.locator('#provider-test').click();await page.waitForFunction(()=>document.querySelector('#provider-result').textContent==='Connection verified');
 await page.screenshot({path:'artifacts/SubForge-017-openai.png'});
 await page.locator('#openai-signout').click();await page.waitForFunction(()=>document.querySelector('#openai-account').textContent==='Signed out');assert.equal(await page.locator('#openai-signin').isEnabled(),true);pass('Sign-out clears account status and permits reconnecting');
 await page.locator('#provider-form button[type="submit"]').click();await page.waitForFunction(()=>!document.querySelector('#modal').open);
 await page.locator('[data-add-provider="translation"]').click();await page.locator('[data-provider-field="kind"]').selectOption('openai_oauth');await page.waitForSelector('#openai-signin');assert.equal(await page.locator('[data-provider-field="cost"]').inputValue(),'plan');await page.locator('#provider-form button[type="submit"]').click();await page.waitForFunction(()=>!document.querySelector('#modal').open);pass('Existing configurations can add OpenAI as another provider/account');
 await page.locator('#check-updates').click();await page.waitForFunction(()=>document.querySelector('#update-result').textContent.includes('latest version'));pass('Current version has no install action');
 await page.evaluate(()=>window.fixture.version='0.2.0');await page.locator('#check-updates').click();await page.waitForSelector('#install-update');assert.equal(await page.locator('.release-notes script').count(),0);assert.match(await page.locator('.release-notes').textContent(),/<script>/);pass('New release is installable and release notes are rendered as text');
 await page.locator('#install-update').click();await page.waitForFunction(()=>document.querySelector('#update-result').textContent.includes('active jobs'));const calls=await page.evaluate(()=>window.fixture.calls);const installation=calls.findIndex(c=>c.cmd==='install_update');assert.equal(calls[installation-1].cmd,'save_settings');assert.equal(calls[installation].args.version,'0.2.0');pass('Update saves settings, pins reviewed version, and reports busy workers');
 await page.screenshot({path:'artifacts/SubForge-017-updates.png'});
 await page.evaluate(()=>window.fixture.noInstaller=true);await page.locator('#check-updates').click();await page.waitForFunction(()=>document.querySelector('#update-content').textContent.includes('not available'));assert.equal(await page.locator('#install-update').count(),0);pass('Missing verified release asset disables in-app installation');
 await page.evaluate(()=>window.fixture.checkError=true);await page.locator('#check-updates').click();await page.waitForFunction(()=>document.querySelector('#update-result').textContent.includes('Try again'));pass('GitHub network errors are retryable');
 for(const language of ['en','el','es','fr','de','pt','it','zh-Hans','ko','ja']){await page.locator('[data-field="ui_language"]').selectOption(language);assert(await page.locator('#check-updates').textContent());}
 pass('Settings retain OpenAI/update controls in all ten languages');
 await page.locator('[data-field="ui_language"]').selectOption('en');await page.locator('[data-action="save"]').click();
 await page.evaluate(()=>{const now=Date.now()/1000;window.fixture.jobs=[{id:'progress-fixture',path:'Parallel-demo.mp4',target:'el',status:'running',stage:'translating',progress:.25,provider:'',message:'',output:'',duration:60,attempt:1,started_at:now-10,activity:{},translation_progress:{total:4,completed:1,cues_done:3,total_cues:12,workers:3,active:[{part:2,cues:3,first:4,last:6,key:'request_spacing',args:{seconds:3,interval:15,until:now+3},provider:'Fixture <img src=x onerror=alert(1)>',model:'fixture',request_started:0},{part:3,cues:3,first:7,last:9,key:'provider_attempt',args:{attempt:1,attempts:3,timeout:180},provider:'Other account',model:'fixture',request_started:now-4}]}}];window.fixture.callbacks[window.fixture.events.changed]({event:'changed',payload:null});});
 await page.locator('[data-page="queue"]').click();await page.waitForSelector('.batch-activity');assert.equal(await page.locator('.batch-activity').count(),2);assert.equal(await page.locator('.batch-activity img').count(),0);const before=await page.locator('[data-wait-key]').textContent();await page.waitForFunction(old=>document.querySelector('[data-wait-key]')?.textContent!==old,before);assert.match(await page.locator('[data-request-start]').textContent(),/Waiting for reply/);assert.equal(await page.locator('progress').getAttribute('value'),'0.25');pass('Per-part progress keeps a determinate total, escapes provider text, and updates wait/reply timers without new events');
 assert.deepEqual(errors,[]);fs.writeFileSync('artifacts/features-regression.json',JSON.stringify({passed:checks.length,checks,pageErrors:errors},null,2));
}finally{await browser.close();}
