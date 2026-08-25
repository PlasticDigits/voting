export function shortenAddress(address: string, left = 6, right = 6): string {
  if (address.length <= left + right + 1) return address
  return `${address.slice(0, left)}…${address.slice(-right)}`
}

export function formatCl8y(raw: string | bigint, decimals = 18): string {
  const value = typeof raw === 'string' ? BigInt(raw || '0') : raw
  const base = 10n ** BigInt(decimals)
  const whole = value / base
  const frac = value % base
  if (frac === 0n) return whole.toString()
  const fracStr = frac.toString().padStart(decimals, '0').replace(/0+$/, '')
  return `${whole}.${fracStr}`
}
