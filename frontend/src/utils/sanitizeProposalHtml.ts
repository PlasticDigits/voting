import DOMPurify from 'dompurify'

/** Display sanitation only. Signed bytes come from /v1/canonicalize. */
export function sanitizeProposalHtml(raw: string): string {
  if (typeof raw !== 'string' || raw.length > 256 * 1024 || new TextEncoder().encode(raw).length > 256 * 1024) return ''
  if (typeof window === 'undefined' || typeof DOMParser === 'undefined' || !DOMPurify.isSupported) return ''
  return DOMPurify.sanitize(raw, {
    ALLOWED_TAGS: ['p', 'br', 'strong', 'b', 'em', 'i', 'u', 'h1', 'h2', 'h3', 'ul', 'ol', 'li', 'blockquote', 'code', 'pre', 'a'],
    ALLOWED_ATTR: ['href', 'title'],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto):|[^a-z]|[a-z+.-]+(?:[^a-z+.-:]|$))/i,
    SANITIZE_NAMED_PROPS: true,
    RETURN_TRUSTED_TYPE: false,
  })
}

export async function sha256Hex(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text)
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}
