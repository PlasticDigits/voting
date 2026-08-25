export function humanizeUserFacingError(raw: string): string {
  const trimmed = raw.trim()
  if (!trimmed) return 'Something went wrong. Please try again.'
  return trimmed
}
