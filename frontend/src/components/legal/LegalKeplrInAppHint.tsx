import { useEffect, useState } from 'react'
import { WalletName } from '@goblinhunt/cosmes/wallet'
import { isBrowserWalletExtensionDetected } from '@/services/terraclassic/walletExtensionInstall'
import { getLegalClickwrapClient, getLegalProperty } from '@/utils/legalClickwrap'
import { LEGAL_KEPLR_INAPP_HINT, shouldShowLegalKeplrInAppHint } from '@/utils/legalKeplrInAppHint'
import type { Network } from '@plasticdigits/cl8y-clickwrap'

export default function LegalKeplrInAppHint({
  address,
  network,
}: {
  address: string
  network: Network
}) {
  const [signedLatest, setSignedLatest] = useState<boolean | null>(null)

  useEffect(() => {
    if (network !== 'TerraClassic') {
      setSignedLatest(null)
      return
    }
    let cancelled = false
    getLegalClickwrapClient()
      .getSignatureStatus(getLegalProperty(), 'TERRA_CLASSIC', address)
      .then((status) => {
        if (!cancelled) setSignedLatest(Boolean(status?.signed_latest))
      })
      .catch(() => {
        if (!cancelled) setSignedLatest(null)
      })
    return () => {
      cancelled = true
    }
  }, [address, network])

  if (network !== 'TerraClassic') return null

  const hasKeplrExtension = isBrowserWalletExtensionDetected(WalletName.KEPLR)
  if (!shouldShowLegalKeplrInAppHint({ hasKeplrExtension, signedLatest })) {
    return null
  }

  return (
    <p data-testid="legal-keplr-inapp-hint" className="app-connected-terms-keplr-hint">
      {LEGAL_KEPLR_INAPP_HINT}
    </p>
  )
}
