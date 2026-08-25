/**
 * Client-side strip aligned with operator-voting's ammonia allowlist.
 *
 * The client hashes the HTML it submits. The API hashes that submitted body
 * (not the ammonia output) and stores ammonia-sanitized HTML. This strip is
 * belt-and-suspenders so TipTap does not emit scripts. Prefer simple markup.
 */
const ALLOWED = new Set([
  'P',
  'BR',
  'STRONG',
  'B',
  'EM',
  'I',
  'U',
  'H1',
  'H2',
  'H3',
  'UL',
  'OL',
  'LI',
  'BLOCKQUOTE',
  'CODE',
  'PRE',
  'A',
])

function stripEventHandlers(el: Element): void {
  for (const attr of Array.from(el.attributes)) {
    if (attr.name.toLowerCase().startsWith('on')) {
      el.removeAttribute(attr.name)
    }
  }
}

export function sanitizeProposalHtml(raw: string): string {
  if (typeof DOMParser === 'undefined') {
    return raw
      .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '')
      .replace(/\son\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, '')
  }
  const doc = new DOMParser().parseFromString(`<div id="root">${raw}</div>`, 'text/html')
  const root = doc.getElementById('root')
  if (!root) return ''

  const walk = (node: Node) => {
    const children = Array.from(node.childNodes)
    for (const child of children) {
      if (child.nodeType !== 1) continue
      const el = child as Element
      if (!ALLOWED.has(el.tagName)) {
        const parent = el.parentNode
        while (el.firstChild) parent?.insertBefore(el.firstChild, el)
        parent?.removeChild(el)
        continue
      }
      stripEventHandlers(el)
      if (el.tagName === 'A') {
        const href = el.getAttribute('href') ?? ''
        if (!/^(https?:|mailto:)/i.test(href)) {
          el.removeAttribute('href')
        }
        el.setAttribute('rel', 'noopener noreferrer')
      }
      walk(el)
    }
  }
  walk(root)
  return root.innerHTML
}

export async function sha256Hex(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text)
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}
