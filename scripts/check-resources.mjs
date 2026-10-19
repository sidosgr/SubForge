import fs from 'node:fs';
const source=fs.readFileSync('src/locales.ts','utf8');
const keys=source.match(/export const keys = \[([^\]]+)\]/)[1].match(/'[^']*'/g);
const records=[...source.matchAll(/(?:^|\n)(?:'([^']+)'|([a-z]{2})):`([^`]+)`\.split\('\|'\)/g)];
if(records.length!==10)throw new Error(`Expected 10 languages, found ${records.length}`);
for(const[,quoted,plain,pack]of records){const lang=quoted??plain;const values=pack.split('|');if(values.length!==keys.length)throw new Error(`${lang}: ${values.length} translations for ${keys.length} keys`);if(values.some(s=>!s.trim()))throw new Error(`${lang}: empty translation`);const file=`public/help/${lang}.html`;if(!fs.existsSync(file))throw new Error(`Missing ${file}`);const html=fs.readFileSync(file,'utf8');if(!html.includes('Sidos - info@sidos.gr')||(html.match(/<section /g)??[]).length<8)throw new Error(`Incomplete guide ${file}`);}
console.log(`${keys.length} interface labels × 10 languages verified; all offline guides present.`);
const providerSource=fs.readFileSync('src/provider-locales.ts','utf8');
const providerKeys=providerSource.match(/const keys=\[([^\]]+)\]/)[1].match(/'[^']*'/g);
const providerRecords=[...providerSource.matchAll(/(?:^|\n)(?:'([^']+)'|([a-z]{2})):`([^`]+)`\.split\('\|'\)/g)];
if(providerRecords.length!==10)throw new Error('Missing provider language');
for(const[,quoted,plain,pack]of providerRecords){if(pack.split('|').length!==providerKeys.length)throw new Error(`Incomplete provider translation: ${quoted??plain}`);}
console.log(`${providerKeys.length} provider setup labels × 10 languages verified.`);
const statusSource=fs.readFileSync('src/status-locales.ts','utf8');
const statusKeys=statusSource.match(/const keys=\[([^\]]+)\]/)[1].match(/'[^']*'/g);
const statusRecords=[...statusSource.matchAll(/(?:^|\n)(?:'([^']+)'|([a-z]{2})):`([^`]+)`\.split\('\|'\)/g)];
if(statusRecords.length!==10)throw new Error('Missing processing status language');
for(const[,quoted,plain,pack]of statusRecords){const values=pack.split('|');if(values.length!==statusKeys.length||values.some(v=>!v.trim()))throw new Error(`Incomplete status translation: ${quoted??plain}`);const en=statusRecords.find(r=>r[2]==='en')[3].split('|');for(let i=0;i<values.length;i++){const tokens=v=>[...v.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort().join(',');if(tokens(values[i])!==tokens(en[i]))throw new Error(`Status placeholders differ: ${quoted??plain} / ${statusKeys[i]}`);}}
console.log(`${statusKeys.length} processing/log labels × 10 languages verified.`);
