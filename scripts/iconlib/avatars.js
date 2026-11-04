// 用 DiceBear 的 CC0 头像风格离线生成一批彩色头像 SVG，输出 JSON：[{id, style, svg}]
const { createAvatar } = require('@dicebear/core');
const styles = {
  notionists: require('@dicebear/notionists'),
  lorelei: require('@dicebear/lorelei'),
  'open-peeps': require('@dicebear/open-peeps'),
};
const bg = ['b6e3f4', 'c0aede', 'd1d4f9', 'ffd5dc', 'ffdfbf', 'c7f0d8', 'fde68a', 'e0e7ff'];
const seeds = ['Felix', 'Aneka', 'Luna', 'Leo', 'Mia', 'Max', 'Zoe', 'Sam', 'Ivy', 'Kai', 'Nova', 'Theo', 'Ruby', 'Jack', 'Lily', 'Oscar'];
const out = [];
for (const [name, style] of Object.entries(styles)) {
  seeds.forEach((seed, i) => {
    const svg = createAvatar(style, { seed, size: 64, radius: 50, backgroundColor: [bg[i % bg.length]] }).toString();
    out.push({ id: `${name}-${i + 1}`, style: name, svg });
  });
}
process.stdout.write(JSON.stringify(out));
