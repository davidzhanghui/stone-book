#!/usr/bin/env python3
"""生成画板「素材」面板的图标素材包（assets/iconlib/*.json.z）。

素材来源（均为可商用的开源许可）：
  - IconPark       Apache-2.0  npm @iconify-json/icon-park-outline + IconPark source/icons.json（中文名 / 分类 / 标签）
  - Tabler Icons   MIT                 npm @iconify-json/tabler + @tabler/icons（分类 / 标签）
  - Fluent Emoji   微软      MIT       npm @iconify-json/fluent-emoji-flat + cldr-annotations-full（中文名）+ unicode.org emoji-test.txt（排序）
  - 技术图标 logos  CC0-1.0             npm @iconify-json/logos（商标归各自所有者）
  - 用户头像        CC0-1.0             DiceBear notionists / lorelei / open-peeps（avatars.js 离线生成）
  - 项目管理        本项目自绘

用法（先下载、解包到 SRC 目录，目录结构见 SRC_* 常量）：
  python3 scripts/iconlib/gen.py <SRC 目录> <avatars.json>
"""

import json
import math
import re
import sys
import zlib
from pathlib import Path

SRC = Path(sys.argv[1])
AVATARS = Path(sys.argv[2])
OUT = Path(__file__).resolve().parents[2] / "assets" / "iconlib"


def iconify(name):
    d = SRC / f"iconify-json-{name}" / "package"
    return json.loads((d / "icons.json").read_text()), json.loads((d / "metadata.json").read_text()) if (d / "metadata.json").exists() else {}


def size_of(j, ic):
    return ic.get("width", j.get("width", 24)), ic.get("height", j.get("height", 24))


# 素材面板卡片上展示的代表性图标（不存在的会被跳过）
PREVIEW = {
    "park": "user peoples home setting-two search like star message mail phone-telephone calendar alarm-clock folder-open file-text chart-line cloud-storage",
    "tabler": "user users home settings search heart star message mail phone calendar clock folder file-text chart-bar cloud database server device-laptop lock",
    "fluent": "grinning-face face-with-tears-of-joy smiling-face-with-heart-eyes thinking-face partying-face smiling-face-with-sunglasses loudly-crying-face thumbs-up "
              "clapping-hands folded-hands red-heart fire party-popper rocket light-bulb check-mark-button",
    "logos": "aws docker kubernetes mysql redis kafka nginx postgresql mongodb java python go react vue github spring",
    "avatar": " ".join(f"{s}-{i}" for i in range(1, 7) for s in ("notionists", "lorelei", "open-peeps")),
}


def write(pack):
    names = {i["n"] for c in pack["cats"] for i in c["icons"]}
    pack["preview"] = [n for n in PREVIEW.get(pack["id"], "").split() if n in names]
    n = sum(len(c["icons"]) for c in pack["cats"])
    raw = json.dumps(pack, ensure_ascii=False, separators=(",", ":")).encode()
    data = zlib.compress(raw, 9)
    (OUT / f"{pack['id']}.json.z").write_bytes(data)
    print(f"{pack['id']:8} {n:5} 个  {len(raw) / 1024:7.0f} KB → {len(data) / 1024:6.0f} KB")


def icon(n, t, k, body, w, h, dw, dh, color=None):
    d = {"n": n, "t": t, "k": k, "b": body}
    if (w, h) != (dw, dh):
        d["w"], d["h"] = w, h
    if color:
        d["c"] = color
    return d


# ---------------------------------------------------------------- IconPark
def gen_park(zh_words):
    j, _ = iconify("icon-park-outline")
    meta = json.loads((SRC / "iconpark-icons.json").read_text())
    order = ["Base", "Edit", "Office", "Arrows", "Charts", "Datas", "Hardware", "Connect", "Communicate", "Operate", "Safe", "Money", "Time", "Travel", "Build",
             "Peoples", "Character", "Hands", "Emoji", "Health", "Life", "Foods", "Clothes", "Makeups", "Baby", "Animals", "Sports", "Game", "Music", "Weather",
             "Energy", "Industry", "Measurement", "Graphics", "Components", "Abstract", "Brand", "Constellation", "Others"]
    cats, cn = {}, {}
    for m in meta:
        name = m["name"]
        ic = j["icons"].get(name)
        if not ic:
            continue
        cn[m["category"]] = m["categoryCN"]
        w, h = size_of(j, ic)
        keys = " ".join([name, *m.get("tag", [])])
        cats.setdefault(m["category"], []).append(icon(name, m["title"], keys, ic["body"], w, h, 48, 48))
        # 英文名 → 中文，给 Tabler 等只有英文名的图标库补中文搜索词
        zh_words.setdefault(name, m["title"])
    out = [{"name": cn[c], "icons": cats[c]} for c in order if c in cats]
    out += [{"name": cn[c], "icons": v} for c, v in cats.items() if c not in order]
    write({"id": "park", "title": "IconPark", "mono": True, "w": 48, "h": 48, "license": "Apache-2.0", "url": "https://www.npmjs.com/package/@iconify-json/icon-park-outline", "cats": out})


# ---------------------------------------------------------------- Tabler
TABLER_CN = [
    ("System", "系统"), ("Arrows", "箭头"), ("Communication", "沟通"), ("Document", "文档"), ("Devices", "设备"), ("Computers", "电脑"), ("Database", "数据库"),
    ("Development", "开发"), ("Version control", "版本控制"), ("Charts", "图表"), ("E-commerce", "电商"), ("Currencies", "货币"), ("Map", "地图"),
    ("Buildings", "建筑"), ("Vehicles", "交通工具"), ("Media", "媒体"), ("Photography", "摄影"), ("Design", "设计"), ("Shapes", "形状"), ("Text", "文本"),
    ("Letters", "字母"), ("Numbers", "数字"), ("Math", "数学"), ("Logic", "逻辑"), ("Symbols", "符号"), ("Badges", "徽章"), ("Health", "健康"),
    ("Sport", "运动"), ("Food", "食物"), ("Nature", "自然"), ("Animals", "动物"), ("Weather", "天气"), ("Mood", "心情"), ("Gestures", "手势"),
    ("Gender", "性别"), ("Games", "游戏"), ("Electrical", "电气"), ("Laundry", "洗涤标识"), ("Zodiac", "星座"), ("Extensions", "文件类型"), ("Brand", "品牌"),
]


def zh_of(name, zh_words):
    """按整名或逐个单词查中文（来自 IconPark 的英文名 → 中文名）。"""
    if name in zh_words:
        return zh_words[name], [zh_words[name]]
    words = [zh_words[w] for w in name.split("-") if w in zh_words]
    return None, words


def gen_tabler(zh_words):
    j, _ = iconify("tabler")
    meta = json.loads((SRC / "tabler" / "package" / "icons.json").read_text())
    cn = dict(TABLER_CN)
    cats = {}
    for name, m in meta.items():
        ic = j["icons"].get(name)
        if not ic or "outline" not in m.get("styles", {}):
            continue
        w, h = size_of(j, ic)
        title, zh = zh_of(name, zh_words)
        keys = " ".join([name, *map(str, m.get("tags", [])), *zh])
        cats.setdefault(m.get("category") or "", []).append(icon(name, title or name, keys, ic["body"], w, h, 24, 24))
    out = [{"name": cn[c], "icons": cats[c]} for c, _ in TABLER_CN if c in cats]
    if "" in cats:
        out.append({"name": "其他", "icons": cats[""]})
    write({"id": "tabler", "title": "Tabler Icons", "mono": True, "w": 24, "h": 24, "license": "MIT", "url": "https://tabler.io/icons", "cats": out})


# ---------------------------------------------------------------- Fluent Emoji
FLUENT_CN = [("Smileys & Emotion", "笑脸与情感"), ("People & Body", "人物与手势"), ("Animals & Nature", "动物与自然"), ("Food & Drink", "食物与饮料"),
             ("Travel & Places", "旅行与地点"), ("Activities", "活动"), ("Objects", "物品"), ("Symbols", "符号"), ("Flags", "旗帜")]
TONE = re.compile(r"-(light|medium-light|medium|medium-dark|dark)(-|$)")


def gen_fluent():
    j, meta = iconify("fluent-emoji-flat")
    chars = json.loads((SRC / "iconify-json-fluent-emoji-flat" / "package" / "chars.json").read_text())
    by_name = {}
    for hexes, name in chars.items():
        by_name.setdefault(name, "".join(chr(int(x, 16)) for x in hexes.split("-")))
    ann = json.loads((SRC / "cldr" / "package" / "annotations" / "zh" / "annotations.json").read_text())["annotations"]["annotations"]
    order = {}
    for line in (SRC / "emoji-test.txt").read_text().splitlines():
        if "; fully-qualified" in line:
            ch = "".join(chr(int(x, 16)) for x in line.split(";")[0].split()).replace("\ufe0f", "")
            order.setdefault(ch, len(order))
    cats = []
    seen = set()
    for c, zh in FLUENT_CN:
        items = []
        for name in meta["categories"].get(c, []):
            # 肤色变体太多，只保留默认肤色
            if name in seen or TONE.search(name) or name not in j["icons"]:
                continue
            seen.add(name)
            ic = j["icons"][name]
            ch = by_name.get(name, "")
            a = ann.get(ch) or ann.get(ch.replace("️", "")) or {}
            title = (a.get("tts") or [name])[0]
            keys = " ".join([name, *a.get("default", [])])
            w, h = size_of(j, ic)
            items.append(icon(name, title, keys, ic["body"], w, h, 32, 32))
        # 按 Unicode emoji-test.txt 的顺序排，同系统表情键盘（😀😃😄…）
        items.sort(key=lambda d: order.get(by_name.get(d["n"], "").replace("\ufe0f", ""), 1 << 30))
        cats.append({"name": zh, "icons": items})
    write({"id": "fluent", "title": "Fluent Emoji", "mono": False, "w": 32, "h": 32, "license": "MIT · Microsoft", "url": "https://github.com/microsoft/fluentui-emoji", "cats": cats})


# ---------------------------------------------------------------- 技术图标
LOGOS = [
    ("云平台", "云 云服务 云计算", "aws microsoft-azure google-cloud cloudflare vercel netlify digital-ocean heroku linode vultr openstack vmware redhat ibm oracle firebase supabase"),
    ("AWS 服务", "亚马逊 云服务", "aws-ec2 aws-s3 aws-lambda aws-rds aws-aurora aws-dynamodb aws-elasticache aws-cloudfront aws-api-gateway aws-route53 aws-vpc aws-elb aws-ecs aws-eks "
                             "aws-fargate aws-sqs aws-sns aws-kinesis aws-msk aws-mq aws-eventbridge aws-step-functions aws-cloudwatch aws-iam aws-cognito aws-kms "
                             "aws-secrets-manager aws-waf aws-shield aws-glue aws-athena aws-redshift aws-open-search aws-cloudformation aws-codepipeline aws-codebuild "
                             "aws-xray aws-amplify aws-appsync aws-ses aws-glacier aws-backup"),
    ("数据库与存储", "数据库 存储 缓存 向量", "mysql postgresql mariadb sqlite mongodb redis valkey memcached elasticsearch opensearch solr cassandra hbase couchbase couchdb neo4j "
                             "influxdb etcd rocksdb snowflake databricks planetscale neon yugabyte singlestore tigergraph surrealdb dgraph milvus qdrant pinecone weaviate prisma"),
    ("中间件与大数据", "中间件 消息队列 网关 大数据", "kafka rabbitmq nats nginx apache-http tomcat envoy traefik-proxy kong consul vault grpc graphql websocket socket-io "
                             "apache-flink apache-spark hadoop airflow presto logstash temporal"),
    ("容器与 DevOps", "容器 运维 持续集成 部署", "docker kubernetes helm rancher openshift terraform ansible puppet chef saltstack vagrant packer pulumi jenkins github-actions "
                             "gitlab circleci travis-ci argo drone teamcity bamboo git github bitbucket nomad linkerd sonarqube jfrog maven gradle npm yarn pnpm homebrew"),
    ("监控与可观测", "监控 日志 告警 链路", "prometheus grafana datadog new-relic sentry kibana splunk dynatrace opentelemetry zabbix pagerduty signoz posthog appdynamics graylog lightstep"),
    ("编程语言", "语言 编程", "java python go rust c c-plusplus c-sharp javascript typescript php ruby kotlin swift scala dart lua perl haskell erlang clojure r-lang julia zig "
                        "bash nodejs deno bun webassembly dotnet"),
    ("前端与客户端", "前端 框架 客户端", "react vue angular svelte nextjs nuxt vite webpack html-5 css-3 tailwindcss bootstrap sass less material-ui ant-design jquery redux "
                              "electron tauri flutter ionic astro remix solidjs threejs d3 storybook jest vitest cypress playwright eslint prettier babel"),
    ("后端框架", "后端 框架 服务端", "spring django flask fastapi express nestjs laravel rails gin koa fastify hibernate quarkus symfony"),
    ("操作系统", "系统", "linux-tux ubuntu debian centos fedora archlinux rocky-linux apple macos microsoft-windows android ios freebsd"),
    ("AI 与数据科学", "人工智能 大模型 机器学习", "openai anthropic claude google-gemini deepseek qwen mistral-ai hugging-face langchain ollama pytorch tensorflow jupyter "
                                   "numpy pandas matplotlib nvidia midjourney stability-ai github-copilot cursor model-context-protocol dify n8n"),
    ("协作与工具", "协作 办公 工具", "slack microsoft-teams zoom notion confluence jira trello asana linear clickup monday airtable figma sketch miro google-drive dropbox "
                              "microsoft-onedrive google-gmail discord telegram whatsapp obsidian postman visual-studio-code intellij-idea vim xcode"),
]


def gen_logos():
    j, _ = iconify("logos")
    cats = []
    for cat, kw, names in LOGOS:
        items = []
        for name in names.split():
            # 有方形图标版本（xxx-icon）时优先用它，更适合放在架构图里
            key = f"{name}-icon" if f"{name}-icon" in j["icons"] else name
            ic = j["icons"].get(key)
            if not ic:
                print("  logos 缺少：", name)
                continue
            w, h = size_of(j, ic)
            items.append(icon(name, name, f"{name} {kw} {cat}", ic["body"], w, h, 256, 256))
        cats.append({"name": cat, "icons": items})
    write({"id": "logos", "title": "技术图标", "mono": False, "w": 256, "h": 256, "license": "CC0-1.0 · gilbarbara/logos（商标归各自所有者）", "url": "https://github.com/gilbarbara/logos", "cats": cats})


# ---------------------------------------------------------------- 用户头像
def gen_avatars():
    data = json.loads(AVATARS.read_text())
    names = {"notionists": "Notionists 线描", "lorelei": "Lorelei 插画", "open-peeps": "Open Peeps 手绘"}
    cats = {}
    for a in data:
        svg = re.sub(r"<metadata>.*?</metadata>", "", a["svg"], flags=re.S)
        m = re.match(r"<svg[^>]*viewBox=\"0 0 ([\d.]+) ([\d.]+)\"[^>]*>(.*)</svg>\s*$", svg, re.S)
        w, h, body = float(m[1]), float(m[2]), m[3]
        # 不同头像的 id 会重名，加上前缀避免导出 SVG 时互相干扰
        body = re.sub(r'id="([^"]+)"', lambda x: f'id="{a["id"]}-{x[1]}"', body)
        body = re.sub(r"url\(#([^)]+)\)", lambda x: f"url(#{a['id']}-{x[1]})", body)
        body = re.sub(r'href="#([^"]+)"', lambda x: f'href="#{a["id"]}-{x[1]}"', body)
        cats.setdefault(a["style"], []).append(icon(a["id"], "头像", f"{a['id']} 头像 用户 人物 avatar", body, w, h, 0, 0))
    write({"id": "avatar", "title": "用户头像", "mono": False, "w": 0, "h": 0, "license": "CC0-1.0 · DiceBear（Notionists / Lorelei / Open Peeps）", "url": "https://www.dicebear.com",
           "cats": [{"name": names[k], "icons": v} for k, v in cats.items()]})


# ---------------------------------------------------------------- 项目管理（自绘，24×24）
RED, ORANGE, YELLOW, GREEN, TEAL, BLUE, PURPLE, GRAY = "#f54a45", "#ff8800", "#ffc60a", "#34c724", "#14c0ff", "#3370ff", "#7f3bf5", "#8f959e"
FONT = 'font-family="Helvetica, Arial, sans-serif" font-weight="bold" text-anchor="middle"'


def badge(color, text, size=13):
    return f'<circle cx="12" cy="12" r="11" fill="{color}"/><text x="12" y="{12 + size * 0.36:.1f}" font-size="{size}" fill="#fff" {FONT}>{text}</text>'


def pill(color, text):
    return f'<rect x="1" y="5" width="22" height="14" rx="7" fill="{color}"/><text x="12" y="15.9" font-size="9.5" fill="#fff" {FONT}>{text}</text>'


def pie(p):
    if p == 0:
        return f'<circle cx="12" cy="12" r="9.5" fill="none" stroke="{BLUE}" stroke-width="2"/>'
    if p == 100:
        return f'<circle cx="12" cy="12" r="11" fill="{GREEN}"/><path d="m7 12.5 3.2 3.2L17 9" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>'
    a = 2 * math.pi * p / 100
    x, y = 12 + 7 * math.sin(a), 12 - 7 * math.cos(a)
    large = 1 if p > 50 else 0
    return (f'<circle cx="12" cy="12" r="9.5" fill="none" stroke="{BLUE}" stroke-width="2"/>'
            f'<path d="M12 12V5A7 7 0 {large} 1 {x:.2f} {y:.2f}Z" fill="{BLUE}"/>')


def flag(color):
    return f'<path d="M5 21V4" stroke="{color}" stroke-width="2" stroke-linecap="round"/><path d="M5 4h13l-3 4.5 3 4.5H5z" fill="{color}"/>'


def star(fill, stroke=None):
    p = "M12 2.5l2.9 6 6.6.9-4.8 4.6 1.2 6.5L12 17.4l-5.9 3.1 1.2-6.5-4.8-4.6 6.6-.9z"
    return f'<path d="{p}" fill="{fill}" stroke="{stroke or fill}" stroke-width="1.2" stroke-linejoin="round"/>'


def medal(color, n):
    return (f'<path d="M7 2h4l2 6H9zM17 2h-4l-2 6h4z" fill="{BLUE}"/><circle cx="12" cy="15" r="7.5" fill="{color}"/>'
            f'<text x="12" y="18.6" font-size="10" fill="#fff" {FONT}>{n}</text>')


def arrow(color, d):
    return f'<circle cx="12" cy="12" r="11" fill="{color}"/><path d="{d}" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>'


def gen_pm():
    cats = [
        ("优先级", [
            ("priority-urgent", "紧急", badge(RED, "!", 15)),
            *[(f"priority-{i}", f"优先级 {i}", badge(c, str(i))) for i, c in enumerate([RED, ORANGE, YELLOW, GREEN, TEAL, BLUE, PURPLE, GRAY, GRAY, GRAY])],
            *[(f"p{i}", f"P{i}", pill(c, f"P{i}")) for i, c in enumerate([RED, ORANGE, YELLOW, BLUE, GRAY])],
        ]),
        ("进度", [(f"progress-{p}", f"进度 {p}%", pie(p)) for p in (0, 25, 50, 75, 100)]),
        ("状态", [
            ("todo", "待开始", f'<circle cx="12" cy="12" r="9.5" fill="none" stroke="{GRAY}" stroke-width="2" stroke-dasharray="3 2.6"/>'),
            ("doing", "进行中", f'<circle cx="12" cy="12" r="9.5" fill="none" stroke="{BLUE}" stroke-width="2"/><path d="M12 5.5a6.5 6.5 0 0 1 0 13z" fill="{BLUE}"/>'),
            ("done", "已完成", pie(100)),
            ("blocked", "已阻塞", f'<circle cx="12" cy="12" r="11" fill="{RED}"/><path d="M7 12h10" stroke="#fff" stroke-width="2.4" stroke-linecap="round"/>'),
            ("paused", "已暂停", f'<circle cx="12" cy="12" r="11" fill="{YELLOW}"/><path d="M9.5 8v8M14.5 8v8" stroke="#fff" stroke-width="2.4" stroke-linecap="round"/>'),
            ("cancelled", "已取消", f'<circle cx="12" cy="12" r="11" fill="{GRAY}"/><path d="m8.5 8.5 7 7m0-7-7 7" stroke="#fff" stroke-width="2.2" stroke-linecap="round"/>'),
            ("risk", "风险", f'<path d="M12 2.5 22.5 21h-21z" fill="{ORANGE}" stroke="{ORANGE}" stroke-width="1.5" stroke-linejoin="round"/><path d="M12 9.5v5" stroke="#fff" stroke-width="2.2" stroke-linecap="round"/><circle cx="12" cy="17.6" r="1.3" fill="#fff"/>'),
            ("question", "疑问", badge(BLUE, "?", 15)),
            ("info", "说明", badge(TEAL, "i", 14)),
        ]),
        ("旗帜", [(f"flag-{n}", f"{zh}旗", flag(c)) for n, zh, c in
                [("red", "红", RED), ("orange", "橙", ORANGE), ("yellow", "黄", YELLOW), ("green", "绿", GREEN), ("blue", "蓝", BLUE), ("purple", "紫", PURPLE), ("gray", "灰", GRAY)]]),
        ("信号灯", [(f"light-{n}", zh, f'<circle cx="12" cy="12" r="10" fill="{c}"/><circle cx="12" cy="12" r="10" fill="none" stroke="#000" stroke-opacity=".08" stroke-width="1"/>')
                 for n, zh, c in [("red", "红灯 延期", RED), ("yellow", "黄灯 有风险", YELLOW), ("green", "绿灯 正常", GREEN), ("blue", "蓝灯", BLUE), ("gray", "灰灯", GRAY)]]),
        ("评价", [
            ("star", "星标", star(YELLOW)),
            ("star-half", "半星", f'<defs><linearGradient id="pm-half"><stop offset=".5" stop-color="{YELLOW}"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>' + star("url(#pm-half)", YELLOW)),
            ("star-empty", "空星", star("none", YELLOW)),
            ("medal-1", "金牌 第一", medal("#f5b400", 1)),
            ("medal-2", "银牌 第二", medal("#a6acb5", 2)),
            ("medal-3", "铜牌 第三", medal("#d4864a", 3)),
            ("thumb-up", "赞", f'<circle cx="12" cy="12" r="11" fill="{BLUE}"/><path d="M8 11v6.5M10.2 11l2.3-4.5c1.2 0 1.8.8 1.6 2L13.7 10H17a1 1 0 0 1 1 1.2l-1 5.3a1.2 1.2 0 0 1-1.2 1H10.2z" fill="#fff"/><path d="M7 11h1.6v6.5H7z" fill="#fff"/>'),
            ("heart", "喜欢", f'<path d="M12 20.5s-8.5-5-8.5-11A4.6 4.6 0 0 1 12 6.8a4.6 4.6 0 0 1 8.5 2.7c0 6-8.5 11-8.5 11" fill="{RED}"/>'),
        ]),
        ("趋势", [
            ("trend-up", "上升 增长", arrow(GREEN, "M12 17V7m-4.5 4.5L12 7l4.5 4.5")),
            ("trend-down", "下降 减少", arrow(RED, "M12 7v10m-4.5-4.5L12 17l4.5-4.5")),
            ("trend-flat", "持平", arrow(GRAY, "M7 12h10m-4.5-4.5L17 12l-4.5 4.5")),
            ("check", "通过", arrow(GREEN, "m7 12.5 3.2 3.2L17 9")),
            ("cross", "不通过", arrow(RED, "m8.5 8.5 7 7m0-7-7 7")),
            ("plus", "新增", arrow(BLUE, "M12 7v10M7 12h10")),
            ("minus", "删除", arrow(ORANGE, "M7 12h10")),
        ]),
    ]
    out = [{"name": c, "icons": [icon(n, t, f"{n} {t} {c}", b, 24, 24, 24, 24) for n, t, b in items]} for c, items in cats]
    write({"id": "pm", "title": "项目管理", "mono": False, "w": 24, "h": 24, "license": "本项目自绘", "url": "", "cats": out})


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    zh_words = {}
    gen_park(zh_words)
    gen_tabler(zh_words)
    gen_pm()
    gen_avatars()
    gen_logos()
    gen_fluent()
