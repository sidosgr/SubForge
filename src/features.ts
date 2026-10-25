import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {featureText} from './features-locales';

const esc=(value:unknown)=>String(value??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const text=(language:string,key:string)=>featureText(language,key)??key;
interface Update {current_version:string;latest_version:string;available:boolean;installable:boolean;notes:string;url:string;size:number}
interface Progress {stage:string;downloaded:number;total:number}
let update:Update|undefined;let progress:Progress={stage:'',downloaded:0,total:0};let updateBusy=false;let updateMessage='';
let getLanguage=()=>'en';
let persistSettings:(()=>Promise<void>)|undefined;
function updateContent(language:string){const t=(key:string)=>text(language,key);return `<p class="hint">${esc(t('updateNotice'))}</p><div class="dialog-actions"><button id="check-updates" ${updateBusy?'disabled':''}>${esc(t('checkUpdates'))}</button>${update?.available&&update.installable?`<button class="accent" id="install-update" ${updateBusy?'disabled':''}>${esc(t('installUpdate'))} · ${Math.ceil(update.size/1048576)} MB</button>`:''}</div><p id="update-result" role="status">${esc(updateMessage?t(updateMessage):'')}${update?` ${esc(update.latest_version)}`:''}</p>${updateBusy&&progress.total?`<progress value="${progress.downloaded}" max="${progress.total}" aria-label="${esc(t('downloading'))}"></progress><p>${Math.round(progress.downloaded/progress.total*100)}%</p>`:''}${update?.available?`${!update.installable?`<p>${esc(t('noInstaller'))}</p>`:''}<details><summary>${esc(t('releaseNotes'))}</summary><pre class="release-notes">${esc(update.notes)}</pre></details>`:''}`;}
export function updatesView(language:string){return `<section class="panel" id="updates"><h2>${esc(text(language,'updates'))}</h2><div id="update-content">${updateContent(language)}</div></section>`;}
function paintUpdates(){const container=document.querySelector('#update-content');if(container){container.innerHTML=updateContent(getLanguage());bindUpdates(getLanguage);}}
export function bindUpdates(language:()=>string,persist?:()=>Promise<void>){getLanguage=language;if(persist)persistSettings=persist;
 const check=document.querySelector<HTMLButtonElement>('#check-updates');if(check)check.onclick=async()=>{if(updateBusy)return;updateBusy=true;updateMessage='checking';progress={stage:'checking',downloaded:0,total:0};paintUpdates();try{update=await invoke<Update>('check_updates');updateMessage=update.available?'available':'upToDate';}catch{updateMessage='updateFailed';}finally{updateBusy=false;paintUpdates();}};
 const install=document.querySelector<HTMLButtonElement>('#install-update');if(install)install.onclick=async()=>{if(updateBusy||!update)return;updateBusy=true;updateMessage='checking';paintUpdates();try{await persistSettings?.();await invoke('install_update',{version:update.latest_version});}catch(error){updateMessage=String(error).includes('update_jobs_active')?'jobsActive':'updateFailed';}finally{updateBusy=false;paintUpdates();}};
}
export async function initFeatures(language:()=>string){getLanguage=language;await listen<Progress>('update-progress',event=>{progress=event.payload;updateMessage=progress.stage==='failed'?'updateFailed':progress.stage;paintUpdates();});}

interface Account {status:string;account:string;welcome:boolean}
export function openAIControls(language:string){const t=(key:string)=>text(language,key);return `<section class="openai-account"><p class="hint">${esc(t('planNotice'))}</p><p id="openai-account" role="status"></p><dialog id="openai-welcome" aria-label="${esc(t('planWelcome'))}"><strong>${esc(t('planWelcome'))}</strong><p>${esc(t('planNotice'))}</p><button type="button" id="openai-got-it">${esc(t('gotIt'))}</button></dialog><div class="dialog-actions"><button type="button" id="openai-signin" class="chatgpt-signin">${esc(t('continueChatGPT'))}</button><button type="button" id="openai-cancel" hidden>${esc(t('cancelSignin'))}</button><button type="button" id="openai-signout" hidden>${esc(t('signOut'))}</button><button type="button" id="openai-usage">${esc(t('manageUsage'))}</button></div></section>`;}
export function openAIError(language:string,error:unknown){const s=String(error);return text(language,s.includes('plan_limit')?'planLimit':s.includes('plan_permission')?'planRequired':s.includes('signin_timeout')?'signinTimeout':'signinFailed');}
export function bindOpenAI(form:HTMLFormElement,id:string,language:string){
 const t=(key:string)=>text(language,key);const account=form.querySelector<HTMLElement>('#openai-account')!;
 let timer:number|undefined;let closed=false;let pending=false;let waiting=false;
 const signin=form.querySelector<HTMLButtonElement>('#openai-signin')!;const signout=form.querySelector<HTMLButtonElement>('#openai-signout')!;const cancel=form.querySelector<HTMLButtonElement>('#openai-cancel')!;
 const update=async()=>{if(closed||!form.isConnected)return;try{const state=await invoke<Account>('openai_status',{id});if(closed)return;const welcome=form.querySelector<HTMLDialogElement>('#openai-welcome')!;if(state.welcome&&!welcome.open)welcome.showModal();const connected=state.status==='connected';const waiting=state.status==='waiting';account.textContent=(connected?t('connected')+' · '+t('chatGPTPlan')+' · '+state.account:waiting?t('waiting'):state.status==='signed_out'?t('signedOut'):openAIError(language,state.status));signin.textContent=t(connected?'reconnect':'continueChatGPT');signout.hidden=!state.account;cancel.hidden=!waiting;signin.disabled=pending||waiting;if(waiting)timer=window.setTimeout(()=>void update(),1000);
 }catch(error){account.textContent=openAIError(language,error);}};
 signin.onclick=async()=>{if(pending)return;pending=true;signin.disabled=true;try{await invoke('openai_signin',{id});waiting=true;await update();}catch(e){waiting=false;account.textContent=openAIError(language,e);}finally{pending=false;signin.disabled=waiting;}};
 cancel.onclick=async()=>{await invoke('openai_cancel',{id});if(timer)window.clearTimeout(timer);await update();};
 signout.onclick=async()=>{if(pending)return;pending=true;signout.disabled=true;try{const revoked=await invoke<boolean>('openai_signout',{id});await update();if(!revoked)account.textContent=t('remoteRevocation');}catch(e){account.textContent=openAIError(language,e);}finally{pending=false;signout.disabled=false;signin.disabled=false;}};
 form.querySelector<HTMLButtonElement>('#openai-usage')!.onclick=()=>void invoke('open_provider_setup',{url:'https://chatgpt.com/settings/usage'}).catch(e=>{account.textContent=openAIError(language,e);});
 const welcome=form.querySelector<HTMLDialogElement>('#openai-welcome')!;const acknowledge=async()=>{await invoke('openai_acknowledge',{id});welcome.close();};form.querySelector<HTMLButtonElement>('#openai-got-it')!.onclick=()=>void acknowledge();welcome.oncancel=e=>{e.preventDefault();void acknowledge();};
 void update();
 return async()=>{closed=true;if(timer)window.clearTimeout(timer);await invoke('openai_cancel',{id});};
}
