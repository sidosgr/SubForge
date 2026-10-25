import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createRequire} from 'node:module';

const require=createRequire(import.meta.url);
const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE||'playwright');
const {version}=JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json','utf8'));
const executable=path.resolve(process.argv[2]||`.tools/installer-${version}-extract/subforge.exe`);
const data=path.resolve(`.tools/release-${version}-smoke-${Date.now()}`);
fs.mkdirSync(data,{recursive:true});
fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({ui_language:'en',target_language:'el',shell_enabled:false}));
const listener=net.createServer();
await new Promise(resolve=>listener.listen(0,'127.0.0.1',resolve));
const port=listener.address().port;
await new Promise(resolve=>listener.close(resolve));
const child=spawn(executable,[],{windowsHide:true,stdio:'ignore',env:{...process.env,SUBFORGE_DATA_DIR:data,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
let browser;
const wait=async(fn)=>{const until=Date.now()+30000;while(Date.now()<until){if(child.exitCode!==null)throw Error('Test app exited');const value=await fn();if(value)return value;await new Promise(resolve=>setTimeout(resolve,100));}throw Error('Release smoke test timed out');};
try{
 browser=await wait(async()=>{try{return await chromium.connectOverCDP(`http://127.0.0.1:${port}`);}catch{return false;}});
 const page=browser.contexts()[0].pages()[0];const errors=[];page.on('pageerror',error=>errors.push(String(error)));
 await page.waitForSelector('#app h1');
 const snapshot=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('snapshot'));
 assert.equal(snapshot.version,version);assert.equal(snapshot.settings.shell_enabled,false);assert.equal(snapshot.settings.request_interval_secs,0);
 await page.locator('[data-page="settings"]').click();
 assert.equal(await page.locator('[data-field="request_interval_secs"]').count(),0);
 const icon=page.locator('[data-field="target_language"]').locator('..').locator('.setting-info');
 await icon.hover();assert(await icon.locator('.setting-tooltip').isVisible());
 await page.locator('[data-action="save"]').click();await page.waitForSelector('#notice:visible');
 assert.match(await page.locator('#notice').textContent(),/saved/i);
 assert.equal(JSON.parse(fs.readFileSync(path.join(data,'settings.json'),'utf8')).request_interval_secs,0);
 fs.mkdirSync('artifacts',{recursive:true});await page.screenshot({path:`artifacts/SubForge-${version}-release-smoke.png`});
 assert.deepEqual(errors,[]);
 const report={version,packagedExecutableStarts:true,isolatedProfile:true,noDefaultPause:true,settingHelpVisible:true,saveConfirmationVisible:true,settingsPersisted:true,pageErrors:errors};
 fs.writeFileSync(`artifacts/release-${version}-smoke.json`,JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));
}finally{
 if(browser)await browser.close();
 if(child.exitCode===null)child.kill();
}
