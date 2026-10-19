from pathlib import Path
from PIL import Image, ImageDraw
root = Path(__file__).resolve().parents[1]
svg = '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><defs><linearGradient id="g" x2="0.7" y2="1"><stop stop-color="#343139"/><stop offset="1" stop-color="#16171b"/></linearGradient><linearGradient id="a" x2="0.8" y2="1"><stop stop-color="#ffd598"/><stop offset="1" stop-color="#ec8935"/></linearGradient></defs><rect x="8" y="8" width="496" height="496" rx="112" fill="url(#g)"/><rect x="36" y="36" width="440" height="440" rx="90" fill="none" stroke="#f7aa55" stroke-opacity=".15" stroke-width="2"/><path d="M120 172h172a32 32 0 0 1 32 32v114a32 32 0 0 1-32 32h-52l-60 42v-42h-60a32 32 0 0 1-32-32V204a32 32 0 0 1 32-32Z" fill="none" stroke="#f7aa55" stroke-width="24" stroke-linejoin="round"/><path d="M132 235h68m-68 55h118" stroke="#e8e5e0" stroke-width="20" stroke-linecap="round"/><path d="m352 94 21 64 63 22-63 22-21 64-22-64-64-22 64-22Z" fill="url(#a)"/><path d="m415 277 9 25 25 9-25 9-9 25-9-25-25-9 25-9Z" fill="#eeb575"/></svg>'''
(root/'public/icon.svg').write_text(svg, encoding='utf-8')
(root/'src-tauri/icons/icon.svg').write_text(svg, encoding='utf-8')
size=1024
im=Image.new('RGBA',(size,size),(0,0,0,0)); d=ImageDraw.Draw(im)
d.rounded_rectangle((16,16,1008,1008),radius=224,fill=(31,31,36),outline=(67,53,43),width=3)
d.rounded_rectangle((72,72,952,952),radius=180,outline=(69,54,42),width=3)
d.rounded_rectangle((176,344,648,700),radius=64,outline=(247,170,85),width=48)
d.polygon([(344,673),(344,784),(480,688)],fill=(247,170,85))
d.rounded_rectangle((264,450,400,490),radius=20,fill=(232,229,224))
d.rounded_rectangle((264,560,500,600),radius=20,fill=(232,229,224))
d.polygon([(704,188),(746,316),(872,360),(746,404),(704,532),(660,404),(532,360),(660,316)],fill=(255,194,123))
d.polygon([(830,554),(848,604),(898,622),(848,640),(830,690),(812,640),(762,622),(812,604)],fill=(238,181,117))
icons=root/'src-tauri/icons';icons.mkdir(exist_ok=True)
for name,s in [('32x32.png',32),('128x128.png',128),('128x128@2x.png',256),('icon.png',512)]:im.resize((s,s),Image.Resampling.LANCZOS).save(icons/name)
im.save(icons/'icon.ico',format='ICO',sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
print('SubForge SVG, PNG and Windows ICO assets generated.')
