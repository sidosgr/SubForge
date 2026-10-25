import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';

const {version,identifier}=JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json','utf8'));
assert.equal(identifier,'gr.sidos.subforge','Release must use the production identifier');
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const setup=fs.readFileSync('artifacts/SubForge-Setup-x64.exe');
assert.deepEqual(setup,fs.readFileSync(`src-tauri/target/release/bundle/nsis/SubForge_${version}_x64-setup.exe`));
assert.equal(fs.readFileSync('artifacts/SHA256SUMS.txt','utf8').trim().split(/\s+/)[0],hash(setup));
const folder=path.resolve(`.tools/installer-${version}-extract`);
fs.mkdirSync(folder,{recursive:true});
const listing=execFileSync('7z.exe',['l','artifacts/SubForge-Setup-x64.exe'],{encoding:'utf8',windowsHide:true});
for(const notice of ['ADDITIONAL-DEPENDENCY-NOTICES.txt','LANGUAGE-DETECTION.txt','FFmpeg-9.0.2-source.tar.gz'])assert(listing.includes(notice),`Missing packaged notice/source: ${notice}`);
execFileSync('7z.exe',['e','-y','artifacts/SubForge-Setup-x64.exe','subforge.exe','-o'+folder],{stdio:'pipe',windowsHide:true});
const packed=fs.readFileSync(path.join(folder,'subforge.exe'));
const release=fs.readFileSync('src-tauri/target/release/subforge.exe');
// Tauri stamps the installer bundle type into the packaged executable.
const normal=bytes=>Buffer.from(bytes.toString('latin1').replaceAll('__TAURI_BUNDLE_TYPE_VAR_NSS','__TAURI_BUNDLE_TYPE_VAR_UNK'),'latin1');
assert.deepEqual(normal(packed),normal(release));
assert(packed.includes(Buffer.from(identifier)),'Packaged executable must use production app data');
assert(!packed.includes(Buffer.from('gr.sidos.subforge.processing-regression')),'Test app must not be packaged');
fs.copyFileSync('artifacts/SubForge-Setup-x64.exe',`artifacts/SubForge-${version}-Setup-x64.exe`);
fs.writeFileSync(`artifacts/SubForge-${version}-SHA256SUMS.txt`,`${hash(setup)}  SubForge-${version}-Setup-x64.exe\n`);
const report={version,bytes:setup.length,sha256:hash(setup),embeddedExecutableMatchesRelease:true,checksumsMatch:true,productionIdentifier:true,additionalDependencyNotices:true};
fs.writeFileSync(`artifacts/installer-${version}-validation.json`,JSON.stringify(report,null,2)+'\n');
console.log(JSON.stringify(report,null,2));
