# 画板素材来源与许可

`*.json.z` 由 `scripts/iconlib/gen.py` 从下列开源项目生成（zlib 压缩的 JSON），仅做格式转换、筛选和排序，未修改图形。
各项目的版权声明与许可全文见同目录的 `THIRD_PARTY_LICENSES.txt`，`scripts/build.sh` 会把它复制到 `石渠.app/Contents/Resources/` 随安装包分发。

| 素材集（id） | 来源 | 许可 |
|---|---|---|
| 常用 | [Lucide](https://lucide.dev)（内置于 `src/board/icons.rs`） | ISC |
| IconPark（`park`）/ IconPark 实心（`park-solid`）/ IconPark 双色（`park-twotone`） | [bytedance/IconPark](https://github.com/bytedance/IconPark)，经 Iconify `@iconify-json/icon-park-outline` / `-solid` / `-twotone` 1.2.4 | Apache-2.0 |
| Tabler Icons（`tabler`）/ Tabler 实心（`tabler-filled`） | [tabler/tabler-icons](https://github.com/tabler/tabler-icons)，经 Iconify `@iconify-json/tabler`；实心版 1.2.43 + `@tabler/icons` 3.50.0 | MIT |
| Phosphor 实心（`ph-fill`）/ Phosphor 双色（`ph-duotone`） | [phosphor-icons/core](https://github.com/phosphor-icons/core)，经 Iconify `@iconify-json/ph` 1.2.2；分类与标签来自 `@phosphor-icons/core` 2.1.1 | MIT |
| TDesign（`tdesign`） | [Tencent/tdesign-icons](https://github.com/Tencent/tdesign-icons)，经 Iconify `@iconify-json/tdesign` 1.2.20；中文分类与关键词来自 `tdesign-icons-view` 0.5.15 | MIT |
| Fluent 图标（`fluentui`） | [microsoft/fluentui-system-icons](https://github.com/microsoft/fluentui-system-icons)，经 Iconify `@iconify-json/fluent` 1.2.61，只收 24px filled | MIT |
| Carbon 架构（`carbon`） | [IBM Carbon](https://github.com/carbon-design-system/carbon)，经 Iconify `@iconify-json/carbon` 1.2.29；只挑云 / 网络 / 数据 / 流程等类别，不收 IBM 产品图标与 logo | Apache-2.0 |
| 品牌图标（`brands`） | [Simple Icons](https://simpleicons.org)，经 Iconify `@iconify-json/simple-icons` 1.2.101，精选约 140 个（技术图标里已有的不重复收） | CC0-1.0（图形数据）；各品牌标志的商标权归其所有者，仅用于指代相应品牌 |
| 国旗与地区旗（`flags`） | [HatScripts/circle-flags](https://github.com/HatScripts/circle-flags)，经 Iconify `@iconify-json/circle-flags` 1.2.14；中文名与大洲来自 Unicode CLDR 49（`cldr-localenames-full` / `cldr-core`）。只收 CLDR 国家 / 地区代码，香港、澳门称「中国香港」「中国澳门」，不收台湾地区旗帜 | MIT；CLDR 为 Unicode License v3 |
| Fluent Emoji（`fluent`） | [microsoft/fluentui-emoji](https://github.com/microsoft/fluentui-emoji)（Flat 风格），经 Iconify `@iconify-json/fluent-emoji-flat`；中文名来自 Unicode CLDR（Unicode License） | MIT |
| 技术图标（`logos`） | [gilbarbara/logos](https://github.com/gilbarbara/logos)，经 Iconify `@iconify-json/logos` | CC0-1.0（图形数据）；各品牌标志的商标权归其所有者，仅用于指代相应产品 |
| 用户头像（`avatar`） | [DiceBear](https://www.dicebear.com) 的 Notionists（Zoish）、Lorelei（Lisa Wischofsky）、Open Peeps（Pablo Stanley）风格 | CC0-1.0 |
| 项目管理（`pm`） | 本项目自绘 | 随本项目 |

素材集 id 会作为图标名前缀（如 `fluentui:person`）存进用户文档，已发布的 id 不能改名或复用。

## 重新生成

```sh
# 在临时目录下载并解包（目录名去掉版本号，如 iconify-json-ph/package/…；@tabler/icons 解包到 tabler/）
npm pack @iconify-json/icon-park-outline @iconify-json/icon-park-solid @iconify-json/icon-park-twotone \
  @iconify-json/tabler @tabler/icons @iconify-json/ph @phosphor-icons/core \
  @iconify-json/tdesign tdesign-icons-view @iconify-json/fluent @iconify-json/carbon \
  @iconify-json/simple-icons @iconify-json/circle-flags cldr-localenames-full cldr-core \
  @iconify-json/fluent-emoji-flat @iconify-json/logos cldr-annotations-full
# 从 IconPark 的 source/icons.json 另存为 iconpark-icons.json
curl -LO https://unicode.org/Public/emoji/latest/emoji-test.txt
# 头像：npm i @dicebear/core @dicebear/collection 后
NODE_PATH=<node_modules> node scripts/iconlib/avatars.js > avatars.json
python3 scripts/iconlib/gen.py <解包目录> avatars.json           # 全部重新生成
python3 scripts/iconlib/gen.py <解包目录> - flags brands         # 只生成指定素材集（不需要头像时传 -）
```

新增素材集时，同步更新 `THIRD_PARTY_LICENSES.txt`（补上该项目的版权声明与许可全文）。
