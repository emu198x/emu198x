import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const siteRoot = dirname(fileURLToPath(import.meta.url)).replace(/\/scripts$/, '');
const repoRoot = join(siteRoot, '..');
const destRoot = join(siteRoot, 'src', 'content', 'docs');

const copyTrees = [
  ['docs/systems', 'systems'],
];

const copyFiles = [
  ['CHANGELOG.md', 'changelog.md'],
  ['docs/status/current-system-usability.md', 'status/current-system-usability.md'],
  ['docs/features/mcp.md', 'features/mcp.md'],
  ['docs/features/scripting.md', 'features/scripting.md'],
  ['docs/features/capture.md', 'features/capture.md'],
  ['docs/features/observability.md', 'features/observability.md'],
];

const excludedPublicDocs = new Set([
  'systems/README.md',
  'systems/rachel-readiness.md',
]);

function ensureDir(path) {
  mkdirSync(path, { recursive: true });
}

function copyFile(sourceRel, targetRel) {
  const source = join(repoRoot, sourceRel);
  if (!existsSync(source)) return;
  const target = join(destRoot, targetRel);
  ensureDir(dirname(target));
  copyFileSync(source, target);
  sanitizePublicMarkdown(target);
}

function publicRelativePath(path) {
  return path.replace(`${repoRoot}/docs/`, '').replace(`${repoRoot}/`, '');
}

function shouldCopyPublicDoc(path) {
  if (path.endsWith('.DS_Store')) return false;
  return !excludedPublicDocs.has(publicRelativePath(path));
}

function sanitizePublicMarkdown(path) {
  let text = readFileSync(path, 'utf8');

  text = text
    .replace(/per \[`knowledge\/decisions\/versioning-milestones\.md`\]\([^)]*\),\n/g, '')
    .replace(/- A unified launcher \(deliberately not built[\s\S]*?\)\n(?=\n|###)/g, '')
    .replace(/- \[`RULES\.md`\]\([^)]*\) — binding architectural constraints\n/g, '')
    .replace(/- \[`knowledge\/decisions\/`\]\([^)]*\) — 30\+ binding decision\n  records covering accuracy bar, chip interfaces, ULA model, snapshot format,\n  versioning strategy, debugger architecture, and more\n/g, '')
    .replace(/- Project lives under the umbrella `198x\/` family alongside Code198x; the\n  two are sibling projects per\n  \[`..\/decisions\/sibling-project-coordination\.md`\]\([^)]*\)\.\n/g, "- Project lives in the 198x family alongside Code Like It's 198x.\n")
    .replaceAll('Code198x', "Code Like It's 198x")
    .replaceAll('October SOLID bar', 'public readiness work')
    .replaceAll('Spectrum SOLID bar met', 'Spectrum support is mature')
    .replaceAll('Spectrum SOLID engineering bar is met', 'Spectrum support is mature')
    .replaceAll('ZX Spectrum SOLID engineering bar is met', 'ZX Spectrum support is mature')
    .replaceAll('SOLID engineering bar', 'public readiness target')
    .replaceAll('SOLID bar', 'public readiness target')
    .replaceAll('SOLID criteria', 'public readiness criteria')
    .replaceAll('SOLID criterion', 'public readiness criterion')
    .replaceAll('SOLID variant', 'mainstream Spectrum variant')
    .replaceAll('SOLID variants', 'mainstream Spectrum variants')
    .replaceAll('SOLID 8', 'mainstream Spectrum variants')
    .replaceAll('engineering-bar machines', 'most mature machine families')
    .replaceAll('engineering bar machines', 'most mature machine families')
    .replaceAll('engineering bar', 'quality target')
    .replaceAll('accuracy bar', 'accuracy target')
    .replaceAll('RULES.md', 'project guidelines')
    .replaceAll('RULES §51-64', 'the project timing model')
    .replaceAll('RULES §62', 'the project timing model')
    .replaceAll('RULES §53–56', 'the project timing model')
    .replaceAll('RULES §53-56', 'the project timing model')
    .replaceAll('RULES §62?', 'project timing model?')
    .replace(/\s*\(RULES\s+§[^)]*\)/g, '')
    .replace(/\s*—\s*see RULES\s+§[^).]*(?=[).])/g, '')
    .replace(/\s*—\s*see `knowledge\/decisions\/[^`]+`/g, '')
    .replace(/\[`knowledge\/decisions\/([^`]+)`\]\([^)]*\)/g, '$1')
    .replace(/`knowledge\/decisions\/([^`]+)`/g, '$1')
    .replace(/\[([^\]]+)\]\((?:\.\.\/)+status\/outstanding-work\.md\)/g, '$1')
    .replace(/\[([^\]]+)\]\((?:\.\.\/)+plans\/[^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\((?:\.\.\/)+(?:\.\.\/)*knowledge\/[^)]*\)/g, '$1')
    .replace(/\[`docs\/plans\/([^`]+)`\]\([^)]*\)/g, '$1');

  writeFileSync(path, text);
}

rmSync(destRoot, { recursive: true, force: true });
ensureDir(destRoot);

for (const [sourceRel, targetRel] of copyTrees) {
  const source = join(repoRoot, sourceRel);
  if (!existsSync(source)) continue;
  cpSync(source, join(destRoot, targetRel), {
    recursive: true,
    filter: shouldCopyPublicDoc,
  });
}

for (const [sourceRel, targetRel] of copyFiles) {
  copyFile(sourceRel, targetRel);
}

for (const tree of copyTrees.map(([, targetRel]) => join(destRoot, targetRel))) {
  sanitizeTree(tree);
}

function sanitizeTree(dir) {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      sanitizeTree(path);
    } else if (path.endsWith('.md')) {
      sanitizePublicMarkdown(path);
    }
  }
}
