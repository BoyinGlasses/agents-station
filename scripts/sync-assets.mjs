#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');

function parseFromArg(argv) {
  const idx = argv.indexOf('--from');
  if (idx !== -1 && argv[idx + 1]) return path.resolve(argv[idx + 1]);
  const eq = argv.find((a) => a.startsWith('--from='));
  if (eq) return path.resolve(eq.slice('--from='.length));
  return path.resolve(repoRoot, '..', 'my-station');
}

function copyTree(src, dst) {
  if (!fs.existsSync(src)) {
    throw new Error(`Missing oracle source directory: ${src}`);
  }
  fs.mkdirSync(dst, { recursive: true });
  let count = 0;
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, entry.name);
    const d = path.join(dst, entry.name);
    if (entry.isDirectory()) {
      count += copyTree(s, d);
    } else {
      fs.copyFileSync(s, d);
      count += 1;
    }
  }
  return count;
}

const oracleRoot = parseFromArg(process.argv.slice(2));
const assetsRoot = path.join(repoRoot, 'assets');

const copies = [
  [path.join(oracleRoot, 'public', 'assets', 'brands'), path.join(assetsRoot, 'brands')],
  [path.join(oracleRoot, 'public', 'assets', 'thumbs'), path.join(assetsRoot, 'thumbs')],
  [path.join(oracleRoot, 'public', 'assets', 'vehicles'), path.join(assetsRoot, 'vehicles')],
  [path.join(oracleRoot, 'src', 'i18n'), path.join(assetsRoot, 'i18n')],
];

let copied = 0;
for (const [src, dst] of copies) {
  copied += copyTree(src, dst);
}

const srcFonts = path.join(oracleRoot, 'src', 'styles', 'fonts');
const dstFonts = path.join(assetsRoot, 'fonts');
execFileSync(
  'cargo',
  ['run', '-p', 'station-ui', '--bin', 'woff2_to_ttf', '--', srcFonts, dstFonts],
  { cwd: repoRoot, stdio: 'inherit' },
);

const ttfList = fs
  .readdirSync(dstFonts)
  .filter((f) => f.endsWith('.ttf'))
  .sort();

const manifest = {
  source: oracleRoot,
  fonts: ttfList,
  copiedFiles: copied,
};
fs.writeFileSync(
  path.join(dstFonts, 'manifest.json'),
  JSON.stringify(manifest, null, 2) + '\n',
);

console.log(`Synced ${copied} assets/i18n files and ${ttfList.length} TTF fonts.`);
