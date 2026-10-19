from pathlib import Path
import json, tomllib, html

root=Path(__file__).resolve().parent.parent
registry=root/'.tools/cargo/registry/src'
lock=tomllib.loads((root/'src-tauri/Cargo.lock').read_text(encoding='utf-8'))
entries=[]
for package in lock['package']:
    name,version=package['name'],package['version']
    locations=list(registry.glob(f'*/{name}-{version}')) if registry.exists() else []
    if not locations: continue
    path=locations[0]
    meta=tomllib.loads((path/'Cargo.toml').read_text(encoding='utf-8')).get('package',{})
    texts=[]
    for file in path.iterdir():
        if file.is_file() and file.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE')) and file.stat().st_size<300000:
            texts.append(file.name+'\n'+file.read_text(encoding='utf-8',errors='replace'))
    entries.append(f"{name} {version} â€” {meta.get('license','See upstream license')}\n{meta.get('repository','')}\n"+'\n'.join(texts))
for name in ['@tauri-apps/api','@tauri-apps/plugin-dialog']:
    path=root/'node_modules'/name
    meta=json.loads((path/'package.json').read_text(encoding='utf-8'))
    texts=[]
    for file in path.glob('LICENSE*'):
        texts.append(file.read_text(encoding='utf-8',errors='replace'))
    entries.append(f"{name} {meta['version']} â€” {meta.get('license','See upstream')}\n"+'\n'.join(texts))
media=root/'src-tauri/resources/licenses'
intro='''SubForge 0.1.6
Sidos - info@sidos.gr

Third-party software retains its original copyright and license.
FFmpeg/FFprobe: Gyan static full build 9.0.2, GPL-3.0; source revision 946fcce07b.
Exact FFmpeg source archive is included in the installed licenses directory.
The supplier's build configuration and external-library version manifest are included in FFmpeg-BUILD.txt.
Upstream binary/source distribution: https://www.gyan.dev/ffmpeg/builds/
FFmpeg source: https://github.com/FFmpeg/FFmpeg/commit/946fcce07b
whisper.cpp linked runtime: 1.9.4, MIT; https://github.com/ggml-org/whisper.cpp/tree/v1.9.4
Whisper Base/Small optional models: MIT; https://github.com/openai/whisper
Models are not included in the installer; downloads are pinned and SHA-256 verified.
Microsoft WebView2 Runtime is supplied by Microsoft under its own license.
OpenCode, Antigravity and Pi are optional official runtime downloads managed by SubForge. They are not inside this installer and retain their upstream terms.
SubForge does not represent or own any of these third-party services or projects.

'''
extra='\n\n'.join(f.name+'\n'+f.read_text(encoding='utf-8',errors='replace') for f in media.glob('*.txt') if f.name!='THIRD-PARTY-NOTICES.txt')
notices=intro+extra+'\n\n'+'\n\n'.join(entries)
(media/'THIRD-PARTY-NOTICES.txt').write_text(notices,encoding='utf-8')
(root/'public/licenses.html').write_text('<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width"><title>SubForge â€” Third-party notices</title><style>body{font:14px/1.7 Segoe UI,Arial;margin:30px;max-width:1000px;color-scheme:light dark}pre{white-space:pre-wrap;overflow-wrap:anywhere}summary{cursor:pointer}a{color:#dd984e}</style></head><body><h1>SubForge</h1><p><strong>Sidos - info@sidos.gr</strong></p><p>Third-party copyright and licenses. Exact media source and build manifest are installed in the applicationâ€™s licenses folder.</p><p><a href="help/en.html">Help</a></p><details><summary>Copyright and full license texts</summary><pre>'+html.escape(notices)+'</pre></details></body></html>',encoding='utf-8')
print(f'{len(entries)} dependency notices generated; upstream licenses preserved.')
