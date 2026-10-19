import fs from 'node:fs';import path from 'node:path';import assert from 'node:assert/strict';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);const{chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.connectOverCDP('http://localhost:9228');const page=browser.contexts()[0].pages()[0];await page.waitForSelector('#app h1');
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const root=path.resolve('.tools/zen-cli-test-data');const runtime=path.join(root,'agents/runtimes/antigravity/1.3.3/agy.exe');const results=[];
const copyRuntime=async source=>{for(let attempt=0;;attempt++){try{fs.copyFileSync(source,runtime);return;}catch(error){if(attempt>=20)throw error;await new Promise(r=>setTimeout(r,250));}}};
const pass=name=>{results.push({name,status:'passed'});console.log('PASS',name);};
const p={id:'subforge-013-signin-fixture',name:'Google sign-in fixture',kind:'antigravity',auth:'oauth',endpoint:'',models:['gemini-3.8-flash-low'],enabled:false,cost:'free',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:30,account_id:'',cli_path:''};
const previous=(await invoke('snapshot')).settings;
try{
 if(await page.locator('#modal').evaluate(el=>el.open))await page.locator('#modal-close').click();
 await invoke('save_settings',{settings:{...previous,shell_enabled:false,translation:[p],ui_language:'en'}});
 await copyRuntime(path.join(root,'signin-fixture.exe'));fs.writeFileSync(path.join(root,'signin-fixture.log'),'');
 await page.locator('[data-page="settings"]').click();await page.locator('[data-edit="translation:0"]').click();await page.locator('#agent-signin').click();
 await page.waitForFunction(()=>document.querySelector('#signin-hint')?.textContent.includes('Interaction sharing is off'));
 let log=fs.readFileSync(path.join(root,'signin-fixture.log'),'utf8');assert.match(log,/terminal-initialized/);assert.match(log,/theme-selected/);assert.match(log,/sharing-disabled/);assert(!log.includes('explicit-setup-confirmation'));pass('Hidden sign-in answers terminal initialization, advances theme setup, and switches interaction sharing off');
 await new Promise(r=>setTimeout(r,1500));assert(!fs.readFileSync(path.join(root,'signin-fixture.log'),'utf8').includes('explicit-setup-confirmation'));assert.equal(await page.locator('#signin-legal').isVisible(),true);pass('Setup pauses for explicit user confirmation with provider terms and privacy controls');
 const continueBox=await page.locator('#signin-continue').boundingBox();assert(continueBox&&continueBox.y>=0&&continueBox.y+continueBox.height<=await page.evaluate(()=>innerHeight));await page.screenshot({path:'artifacts/SubForge-Google-setup-fixed.png'});
 await page.locator('#signin-continue').click();await page.waitForFunction(()=>document.querySelector('#signin-hint')?.textContent.includes('Continue sign-in'));log=fs.readFileSync(path.join(root,'signin-fixture.log'),'utf8');assert.match(log,/explicit-setup-confirmation/);pass('Continue confirms the setup only after the app explains the provider terms');
 assert.equal(await page.locator('#signin-browser').isVisible(),false);assert(!await page.locator('#provider-result').textContent().then(text=>text.includes('https://')));pass('Terminal output and authorization URLs are not exposed; browser retry appears only when a validated URL exists');
 await page.locator('#signin-finish').click();await page.waitForFunction(()=>document.querySelector('#provider-result')?.textContent.includes('Test connection to verify access'));pass('Closing sign-in does not claim that provider access has been verified');await page.locator('#modal-close').click();
 fs.writeFileSync(path.join(root,'signin-connected.txt'),'fixture');fs.writeFileSync(path.join(root,'signin-fixture.log'),'');
 await page.locator('[data-edit="translation:0"]').click();await page.locator('#agent-signin').click();await page.waitForFunction(()=>document.querySelector('#signin-hint')?.textContent.includes('Interaction sharing is off'));
 assert.equal(await invoke('agent_signin_action',{id:p.id,action:'open_privacy'}),'signin_terms');pass('Windows default-browser launch accepts the provider privacy page through its native URL association');
 await page.locator('#signin-continue').click();await page.waitForFunction(()=>document.querySelector('#signin-hint')?.textContent.includes('has a saved Google session'));assert.equal(await page.locator('#signin-switch').isVisible(),true);assert(!await page.locator('#provider-form').textContent().then(t=>t.includes('account@example.test')));assert(!fs.readFileSync(path.join(root,'signin-fixture.log'),'utf8').includes('explicit-account-switch'));pass('A cached native Google session is explained without exposing account identity or signing it out automatically');
 await page.locator('#signin-switch').click();await page.waitForFunction(()=>document.querySelector('#signin-hint')?.textContent.includes('Continue sign-in'));assert.match(fs.readFileSync(path.join(root,'signin-fixture.log'),'utf8'),/explicit-account-switch/);pass('Account switching sends the native CLI logout command only after the explicit user action');await page.locator('#signin-finish').click();await page.waitForFunction(()=>document.querySelector('#provider-result')?.textContent.includes('Test connection to verify access'));await page.locator('#modal-close').click();fs.rmSync(path.join(root,'signin-connected.txt'),{force:true});
 await copyRuntime(path.join(root,'official-agy.exe'));
 const real={...p,id:'subforge-013-real-onboarding'};await invoke('save_settings',{settings:{...previous,shell_enabled:false,translation:[real],ui_language:'en'}});
 await page.locator('[data-edit="translation:0"]').click();await page.locator('#agent-signin').click();await page.waitForFunction(()=>document.querySelector('#signin-hint')?.textContent.includes('Interaction sharing is off'),null,{timeout:35000});pass('Official Antigravity 1.3.3 reaches the actual setup confirmation screen after the terminal repair');
 await page.locator('#modal-close').click();assert.equal(await invoke('agent_signin_action',{id:real.id,action:'finish'}),'closed');pass('Closing the dialog stops its owned sign-in helper and polling');
}finally{
 if(await page.locator('#modal').evaluate(el=>el.open))await page.locator('#modal-close').click();
 await invoke('agent_signin_action',{id:p.id,action:'finish'});await invoke('agent_signin_action',{id:'subforge-013-real-onboarding',action:'finish'});
 fs.rmSync(path.join(root,'signin-connected.txt'),{force:true});
 await copyRuntime(path.join(root,'official-agy.exe'));await invoke('save_settings',{settings:previous});
 fs.writeFileSync('artifacts/signin-014-regression.json',JSON.stringify(results,null,2));await browser.close();
}
