"""生成草图手写字体 assets/fonts/StonebrookSketch-Regular.ttf.z。

来源（均为 SIL OFL 1.1，许可见 assets/fonts/LICENSE-StonebrookSketch.txt）：
- Excalifont：excalidraw/excalidraw 仓库 packages/excalidraw/fonts/Excalifont/*.woff2（7 个分片）
- 小赖 Xiaolai：https://github.com/lxgw/kose-font/releases/download/v3.126/Xiaolai-Regular.ttf

用法（必须在 venv 里）：
  python -m venv .venv && source .venv/bin/activate && pip install fonttools brotli
  python scripts/fonts/build_sketch_font.py SRC_DIR OUT_DIR
SRC_DIR 里放 ex-*.woff2 与 Xiaolai-Regular.ttf；生成后把 OUT_DIR/StonebrookSketch-Regular.ttf.z 复制到 assets/fonts/。
OFL 不允许修改版沿用保留字体名，所以合并后改名为 Stonebrook Sketch。
"""
import sys, glob, os
from fontTools.ttLib import TTFont
from fontTools import subset
from fontTools.merge import Merger, Options
import zlib
src, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
chunks=[]
for f in sorted(glob.glob(f"{src}/ex-*.woff2")):
    t=TTFont(f); t.flavor=None
    p=f"{out}/{os.path.basename(f)}.ttf"; t.save(p); chunks.append(p)
    print(os.path.basename(f), t['head'].unitsPerEm, 'glyf' in t, len(t.getBestCmap()))
ex=Merger().merge(chunks); ex.save(f"{out}/excalifont.ttf")
excmap=set(TTFont(f"{out}/excalifont.ttf").getBestCmap())
x=TTFont(f"{src}/Xiaolai-Regular.ttf")
print('xiaolai upm', x['head'].unitsPerEm, 'glyf' in x, len(x.getBestCmap()))
# 常用字：GB2312 一二级 + 中文标点/全角
chars=set()
for hi in range(0xB0,0xF8):
    for lo in range(0xA1,0xFF):
        try: chars.add(ord(bytes([hi,lo]).decode('gb2312')))
        except: pass
for hi in range(0xA1,0xAA):
    for lo in range(0xA1,0xFF):
        try: chars.add(ord(bytes([hi,lo]).decode('gb2312')))
        except: pass
chars |= set(range(0x3000,0x3040)) | set(range(0xFF00,0xFFF0))
chars -= excmap
opts=subset.Options(); opts.layout_features=['*']; opts.name_IDs=['*']; opts.notdef_outline=True; opts.hinting=False
s=subset.Subsetter(opts); s.populate(unicodes=sorted(chars)); s.subset(x)
x.save(f"{out}/xiaolai-sub.ttf")
print('sub size', os.path.getsize(f"{out}/xiaolai-sub.ttf"), 'ex size', os.path.getsize(f"{out}/excalifont.ttf"))

w=out
# 去掉两边的 hinting 程序差异
m=Merger(options=Options(drop_tables=['vhea','vmtx','VORG','fpgm','prep','cvt ','gasp','DSIG','GDEF','GPOS','GSUB','meta']))
f=m.merge([f"{w}/excalifont.ttf", f"{w}/xiaolai-sub.ttf"])
name=f['name']
FAM="Stonebrook Sketch"
for rec in list(name.names):
    if rec.nameID in (1,16): rec.string=FAM
    elif rec.nameID==4: rec.string=FAM+" Regular"
    elif rec.nameID==6: rec.string="StonebrookSketch-Regular"
    elif rec.nameID==3: rec.string="1.000;STBK;StonebrookSketch"
    elif rec.nameID in (2,17): rec.string="Regular"
    elif rec.nameID==0: rec.string="Copyright (c) 2024 Excalidraw (Excalifont, OFL-1.1); Copyright lxgw (Xiaolai, OFL-1.1). Subset/merged for Stonebrook."
name.names=[r for r in name.names if r.nameID not in (16,17)]
f.save(f"{w}/StonebrookSketch-Regular.ttf")
b=open(f"{w}/StonebrookSketch-Regular.ttf","rb").read()
z=zlib.compress(b,9); open(f"{w}/StonebrookSketch-Regular.ttf.z","wb").write(z)
t=TTFont(f"{w}/StonebrookSketch-Regular.ttf")
cm=t.getBestCmap()
print(len(b), len(z), len(cm), ord('A') in cm, ord('中') in cm, ord('，') in cm, t['head'].unitsPerEm)
