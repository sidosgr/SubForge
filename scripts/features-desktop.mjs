import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE||'playwright');
const browser=await chromium.connectOverCDP('http://127.0.0.1:9237');
const page=browser.contexts()[0].pages()[0];
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const results=[];const pass=name=>{results.push(name);console.log('PASS',name);};
try{
 await page.waitForSelector('#app h1');
 const snapshot=await invoke('snapshot');assert.equal(snapshot.version,'0.1.7');
 const id='synthetic-openai-017';const account=await invoke('openai_status',{id});assert.deepEqual(account,{status:'signed_out',account:'',welcome:false});
 assert.equal(await invoke('openai_signout',{id}),true);await invoke('openai_cancel',{id});pass('Native OpenAI account commands safely handle an unsigned-in account');
 const provider={id,name:'OpenAI fixture',kind:'openai_oauth',endpoint:'',models:[],enabled:true,cost:'plan',credit_enabled:false,retries:0,delay_ms:0,timeout_secs:10,account_id:'',cli_path:'',auth:'oauth'};
 await assert.rejects(()=>invoke('provider_models',{provider}),/openai_signin_required/);pass('OpenAI model discovery requires the app-owned OAuth session');
 const release=await invoke('check_updates');assert.equal(release.current_version,'0.1.7');assert.equal(release.latest_version,'0.1.6');assert.equal(release.available,false);assert.equal(release.installable,true);assert.equal(release.size,359639686);pass('Native updater reads and validates the live GitHub release and its checksum metadata');
 if(!process.argv.includes('--release')){await assert.rejects(()=>invoke('install_update',{version:'0.1.6'}),/update_requires_installed_build/);pass('Development builds cannot launch an update installer');}
 await page.locator('[data-page="settings"]').click();await page.locator('#check-updates').click();await page.waitForFunction(()=>document.querySelector('#update-result').textContent.includes('latest version'));assert.equal(await page.locator('#install-update').count(),0);pass('Native Settings shows no newer GitHub version and offers no downgrade');
 await page.locator('[data-add-provider="translation"]').click();await page.locator('[data-provider-field="kind"]').selectOption('openai_oauth');await page.waitForSelector('#openai-signin');await page.waitForFunction(()=>document.querySelector('#openai-account').textContent==='Signed out');assert.equal(await page.locator('#provider-key').count(),0);assert.equal(await page.locator('[data-provider-field="endpoint"]').count(),0);pass('Native provider editor exposes Continue with ChatGPT with no API key');
 await page.screenshot({path:process.argv.includes('--release')?'artifacts/SubForge-017-release-openai.png':'artifacts/SubForge-017-native-openai.png'});await page.locator('#modal-close').click();
 fs.writeFileSync(process.argv.includes('--release')?'artifacts/features-release.json':'artifacts/features-desktop.json',JSON.stringify({passed:results.length,checks:results,github:release},null,2));
}finally{await browser.close();}
