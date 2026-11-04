# 画板素材来源与许可

`*.json.z` 由 `scripts/iconlib/gen.py` 从下列开源项目生成（zlib 压缩的 JSON），仅做格式转换、筛选和排序，未修改图形。

| 素材集 | 来源 | 许可 |
|---|---|---|
| 常用 | [Lucide](https://lucide.dev)（内置于 `src/board/icons.rs`） | ISC |
| IconPark | IconPark，经 Iconify `@iconify-json/icon-park-outline` | Apache-2.0 |
| Tabler Icons | [tabler/tabler-icons](https://github.com/tabler/tabler-icons)，经 Iconify `@iconify-json/tabler` | MIT |
| Fluent Emoji | [microsoft/fluentui-emoji](https://github.com/microsoft/fluentui-emoji)（Flat 风格），经 Iconify `@iconify-json/fluent-emoji-flat`；中文名来自 Unicode CLDR（Unicode License） | MIT |
| 技术图标 | [gilbarbara/logos](https://github.com/gilbarbara/logos)，经 Iconify `@iconify-json/logos` | CC0-1.0（图形数据）；各品牌标志的商标权归其所有者，仅用于指代相应产品 |
| 用户头像 | [DiceBear](https://www.dicebear.com) 的 Notionists（Zoish）、Lorelei（Lisa Wischofsky）、Open Peeps（Pablo Stanley）风格 | CC0-1.0 |
| 项目管理 | 本项目自绘 | 随本项目 |

## 重新生成

```sh
# 在临时目录下载并解包（目录名去掉版本号）
npm pack @iconify-json/icon-park-outline @iconify-json/tabler @iconify-json/fluent-emoji-flat @iconify-json/logos @tabler/icons cldr-annotations-full
# 从 IconPark 的 source/icons.json 另存为 iconpark-icons.json
curl -LO https://unicode.org/Public/emoji/latest/emoji-test.txt
# 头像：npm i @dicebear/core @dicebear/collection 后
NODE_PATH=<node_modules> node scripts/iconlib/avatars.js > avatars.json
python3 scripts/iconlib/gen.py <解包目录> avatars.json
```
