import { sanitizeProposalHtml } from '@/utils/sanitizeProposalHtml'
import type { HTMLAttributes } from 'react'

/** The only display boundary permitted to insert proposal HTML into the DOM. */
export default function SafeHtml({
  html,
  ...props
}: Omit<HTMLAttributes<HTMLDivElement>, 'children' | 'dangerouslySetInnerHTML'> & {
  html: string
}) {
  return <div {...props} dangerouslySetInnerHTML={{ __html: sanitizeProposalHtml(html) }} />
}
