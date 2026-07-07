function rewriteMarkdownUrl(url) {
  if (
    !url ||
    url.startsWith('#') ||
    /^[a-z][a-z0-9+.-]*:/i.test(url)
  ) {
    return url;
  }

  const match = url.match(/^([^?#]*)([?#].*)?$/);
  if (!match) return url;

  let [, path, suffix = ''] = match;
  if (!path.endsWith('.md')) return url;

  path = path.replace(/(^|\/)(README|index)\.md$/i, '$1');
  path = path.replace(/\.md$/i, '/');

  return `${path}${suffix}`;
}

function walk(node) {
  if (!node || typeof node !== 'object') return;

  if ((node.type === 'link' || node.type === 'definition') && typeof node.url === 'string') {
    node.url = rewriteMarkdownUrl(node.url);
    if (node.url === 'https://code198x.stevehill.xyz') {
      node.url = 'https://code198x.com';
    }
  }

  if (node.type === 'text' && typeof node.value === 'string') {
    node.value = node.value.replaceAll('Code198x', "Code Like It's 198x");
    node.value = node.value.replaceAll('code198x.stevehill.xyz', 'code198x.com');
  }

  if (Array.isArray(node.children)) {
    for (const child of node.children) {
      walk(child);
    }
  }
}

export function remarkRewriteMarkdownLinks() {
  return (tree) => walk(tree);
}
