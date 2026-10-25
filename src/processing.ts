import {processingText} from './processing-locales';
import {statusTranslation} from './status-locales';
const esc=(s:unknown)=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
interface Batch {part:number;cues:number;first:number;last:number;key:string;args:Record<string,unknown>;provider:string;model:string;request_started:number}
export interface TranslationProgress {total:number;completed:number;cues_done:number;total_cues:number;workers:number;active:Batch[]}
const text=(language:string,key:string,args:Record<string,unknown>={})=>processingText(language,key,args)??statusTranslation(language,key,args)??key;
export function translationActivity(job:{stage:string;status:string;started_at:number;translation_progress?:TranslationProgress},language:string):string {
 const p=job.translation_progress;
 if(!p?.total||job.stage!=='translating'||!['running','paused'].includes(job.status))return '';
 const now=Date.now()/1000;
 const rows=[...p.active].sort((a,b)=>a.part-b.part).map(b=>{
  const args={...b.args};if(Number(args.until))args.seconds=Math.max(0,Math.ceil(Number(args.until)-now));
  return `<div class="batch-activity ${b.key.includes('rate_limit')?'rate-wait':''}"><div class="batch-heading"><strong>${esc(text(language,'batch',{part:b.part,total:p.total}))}</strong><span>${esc(text(language,'batch_range',b as unknown as Record<string,unknown>))}</span></div>${b.provider?`<p class="batch-provider" title="${esc(b.provider+' / '+b.model)}">${esc(b.provider)} · ${esc(b.model)}</p>`:''}<p ${args.until?`data-wait-key="${esc(b.key)}" data-wait-args="${esc(JSON.stringify(b.args))}"`:''}>${esc(text(language,b.key,args))}</p>${args.attempt?`<small>${esc(text(language,'attemptCount',args))}</small>`:''}${b.request_started?`<small data-request-start="${b.request_started}" data-request-timeout="${Number(args.timeout)||0}">${esc(text(language,'request_age',{seconds:Math.max(0,Math.floor(now-b.request_started)),timeout:args.timeout}))}</small>`:''}</div>`;
 }).join('');
 return `<div class="job-status translation-progress"><p>${esc(text(language,'translation_summary',{done:p.completed,total:p.total,cues:p.cues_done,all_cues:p.total_cues}))}</p><div class="status-facts"><span>${esc(text(language,'active_parts',{active:p.active.length,workers:p.workers}))}</span><span data-elapsed="${job.started_at}">${esc(text(language,'elapsed',{seconds:Math.max(0,Math.floor(now-job.started_at))}))}</span><span>${Math.floor(p.completed/p.total*100)}%</span></div><div class="batch-activities">${rows}</div></div>`;
}
export function tickProcessing(language:string) {
 const now=Date.now()/1000;
 document.querySelectorAll<HTMLElement>('[data-wait-key]').forEach(el=>{try{const args=JSON.parse(el.dataset.waitArgs!);args.seconds=Math.max(0,Math.ceil(Number(args.until)-now));el.textContent=text(language,el.dataset.waitKey!,args);}catch{}});
 document.querySelectorAll<HTMLElement>('[data-request-start]').forEach(el=>{el.textContent=text(language,'request_age',{seconds:Math.max(0,Math.floor(now-Number(el.dataset.requestStart))),timeout:el.dataset.requestTimeout});});
}
