"""截图回归：同一份脚本分别用两个版本的二进制跑，逐张逐像素比较截图（思路来自 quiet.md）。

用法：.venv/bin/python tools/harness/compare.py <脚本.txt> <旧二进制> <新二进制> [输出目录]
  输出目录默认 /tmp/stonebrook-compare：a/、b/ 两次运行的截图，有差异时写 diff-<名字>.png
  任何一张不同就以状态 1 退出。只移动代码、不改外观的重构，应当每张都是「一致」。

光标不闪烁（STONEBROOK_NOBLINK）。脚本开头先用 rmove 把指针移到空白处，否则真实鼠标位置会以 hover 形式进截图。
旧版二进制的编法见 tools/harness/README.md。
"""
import os, sys
from PIL import Image, ImageChops

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from run import run  # noqa: E402

# 窗口左上角红黄绿按钮所在区域（2x 截图像素）
IGNORE = (0, 0, 180, 90)

if len(sys.argv) < 4:
    sys.exit(__doc__)
script, bins = sys.argv[1], sys.argv[2:4]
work = os.path.abspath(sys.argv[4]) if len(sys.argv) > 4 else '/tmp/stonebrook-compare'
for f in os.listdir(work) if os.path.isdir(work) else []:
    if f.startswith('diff-'):
        os.remove(os.path.join(work, f))

for side, binary in zip('ab', bins):
    print(f'▸ 运行 {side}：{binary}')
    run(script, os.path.join(work, side), binary)

shots = sorted(f for f in os.listdir(os.path.join(work, 'a')) if f.endswith('.png'))
if not shots:
    sys.exit('a 没有产生任何截图：检查脚本里的 shot 命令和 a/log')
bad = 0
for f in shots:
    pb = os.path.join(work, 'b', f)
    if not os.path.exists(pb):
        print(f'{f}: b 中缺少这张')
        bad += 1
        continue
    a = Image.open(os.path.join(work, 'a', f)).convert('RGB')
    b = Image.open(pb).convert('RGB')
    if a.size != b.size:
        print(f'{f}: 尺寸不同 {a.size} vs {b.size}')
        bad += 1
        continue
    # 左上角红黄绿按钮随窗口是否在前台变色（取决于运行时你在操作什么），与代码无关：不参与比较
    for im in (a, b):
        im.paste((0, 0, 0), IGNORE)
    d = ImageChops.difference(a, b)
    box = d.getbbox()
    if box is None:
        print(f'{f}: 一致')
        continue
    n = sum(1 for p in d.getdata() if p != (0, 0, 0))
    print(f'{f}: {n} 个像素不同，范围 {box}')
    bad += 1
    d.point(lambda v: 255 if v else 0).save(os.path.join(work, f'diff-{f}'))
extra = sorted(set(f for f in os.listdir(os.path.join(work, 'b')) if f.endswith('.png')) - set(shots))
for f in extra:
    print(f'{f}: a 中缺少这张')
    bad += 1
print(f'\n{len(shots) - bad if bad <= len(shots) else 0}/{len(shots)} 张一致' + ('' if not bad else f'，差异图在 {work}'))
sys.exit(1 if bad else 0)
