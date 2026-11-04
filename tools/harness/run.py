"""用临时数据目录脚本驱动一次石渠，并在进程内截图。

用法：.venv/bin/python tools/harness/run.py <脚本.txt> [输出目录]
  输出目录默认 /tmp/stonebrook-run：截图 <名字>.png、日志 log、数据目录 data/（每次清空重建）
  STONEBROOK_BIN 默认本仓库的 target/release/stonebrook（需要带 dev 功能编译：cargo build --release）

不会碰 ~/Stonebrook：数据目录由 STONEBROOK_DATA_DIR 指向输出目录下的 data/。
脚本命令见 src/autotest.rs；脚本末尾自动补一行 quit。
"""
import os, shutil, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
repo = os.path.dirname(os.path.dirname(here))


def run(script_path, work, binary=None, extra_env=None):
    work = os.path.abspath(work)
    os.makedirs(work, exist_ok=True)
    for f in os.listdir(work):
        if f.endswith('.png'):
            os.remove(os.path.join(work, f))
    data = os.path.join(work, 'data')
    shutil.rmtree(data, ignore_errors=True)
    os.makedirs(data)
    script = os.path.join(work, 'script.txt')
    with open(script_path, encoding='utf-8') as src, open(script, 'w', encoding='utf-8') as dst:
        dst.write(src.read() + '\nquit\n')
    binary = os.path.abspath(binary or os.environ.get('STONEBROOK_BIN') or os.path.join(repo, 'target', 'release', 'stonebrook'))
    if not os.path.exists(binary):
        sys.exit(f'找不到 {binary}，先 cargo build --release')
    env = {
        'HOME': os.path.expanduser('~'),
        'PATH': '/opt/homebrew/bin:/usr/bin:/bin',
        'STONEBROOK_DATA_DIR': data,
        'STONEBROOK_SCRIPT': script,
        'STONEBROOK_SHOT_DIR': work,
        'STONEBROOK_FORCE_DRAW': '1',
        'STONEBROOK_NOBLINK': '1',
    }
    env.update(extra_env or {})
    with open(os.path.join(work, 'log'), 'w') as log:
        # 脚本跑完会 quit；给一个上限防止卡死
        subprocess.run([binary], env=env, cwd=work, stdout=log, stderr=subprocess.STDOUT, timeout=600)
    return sorted(f for f in os.listdir(work) if f.endswith('.png'))


if __name__ == '__main__':
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out = sys.argv[2] if len(sys.argv) > 2 else '/tmp/stonebrook-run'
    shots = run(sys.argv[1], out)
    print(f'{len(shots)} 张截图 → {os.path.abspath(out)}')
