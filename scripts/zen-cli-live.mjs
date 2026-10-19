// Synthetic, keyless official-CLI check. Never uses an existing user's provider ID.
import fs from 'node:fs';import path from 'node:path';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);const {chromium}=require(process.env.SUBFORGE_PLAYWRIGHT_MODULE || 'playwright');
const browser=await chromium.connectOverCDP('http://localhost:9228');const page=browser.contexts()[0].pages()[0];
const invoke=(cmd,args={})=>page.evaluate(({cmd,args})=>window.__TAURI_INTERNALS__.invoke(cmd,args),{cmd,args});
const report={runtime:'Official OpenCode CLI 1.19.0',syntheticOnly:true,userCredentialsUsed:false};
const provider={id:'subforge-012-official-keyless-check',name:'Official Zen keyless check',kind:'zen',endpoint:'',models:['space-bunny-free','muse-spark-1.3-contributor-free','mimo-v2.6-flash-free'],enabled:true,cost:'free',credit_enabled:false,retries:0,delay_ms:1000,timeout_secs:90,account_id:'',cli_path:'',auth:'key'};
try {
 if(await invoke('key_saved',{id:provider.id}))throw Error('Unexpected credential on fixture ID');
 const catalog=await invoke('provider_models',{provider});report.catalog=provider.models.map(model=>({model,available:catalog.includes(model)}));console.log('Official CLI catalog:',JSON.stringify(report.catalog));
 try{report.connection=await invoke('test_provider',{provider,role:'translation'});console.log('Official CLI synthetic translation:',report.connection);}catch(e){report.connectionError=String(e);console.log('Official CLI synthetic translation:',report.connectionError.slice(0,1200));}
}finally{fs.writeFileSync('artifacts/zen-cli-live.json',JSON.stringify(report,null,2));await browser.close();}
