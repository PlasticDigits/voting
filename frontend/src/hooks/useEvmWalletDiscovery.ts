import { useMemo } from 'react'
import { useConnectors } from 'wagmi'
import type { Connector } from 'wagmi'

const SIMULATED_PRIORITY = -1
const INJECTED_PRIORITY = 0
const WALLETCONNECT_PRIORITY = 100
const COINBASE_PRIORITY = 200
const OTHER_PRIORITY = 300

function getConnectorPriority(connector: Connector): number {
  const name = (connector.name || '').toLowerCase()
  const id = (connector.uid || '').toLowerCase()
  if (connector.type === 'mock') return SIMULATED_PRIORITY
  if (id.includes('walletconnect') || name.includes('walletconnect')) return WALLETCONNECT_PRIORITY
  if (id.includes('coinbase') || name.includes('coinbase')) return COINBASE_PRIORITY
  if (id.includes('injected') || id.includes('eip6963') || connector.type === 'injected') {
    return INJECTED_PRIORITY
  }
  return OTHER_PRIORITY
}

export function useEvmWalletDiscovery() {
  const connectors = useConnectors()

  const sortedConnectors = useMemo(() => {
    return [...connectors].sort((a, b) => {
      const pa = getConnectorPriority(a)
      const pb = getConnectorPriority(b)
      if (pa !== pb) return pa - pb
      return (a.name || '').localeCompare(b.name || '')
    })
  }, [connectors])

  return { connectors: sortedConnectors }
}
