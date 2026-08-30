import { useEffect, useMemo, useState } from 'react'
import { WalletName } from '@goblinhunt/cosmes/wallet'
import { isBrowserWalletExtensionDetected } from '@/services/terraclassic/walletExtensionInstall'
import { CopyButton } from '@/components/ui/CopyButton'
import { useEvmWalletStore } from '@/stores/evmWallet'
import {
  getLegalClickwrapClient,
  getLegalProperty,
  resolveLegalRedirectUri,
} from '@/utils/legalClickwrap'
import { LEGAL_KEPLR_INAPP_HINT, shouldShowLegalKeplrInAppHint } from '@/utils/legalKeplrInAppHint'
import {
  buildLegalEvmSignUrl,
  buildMetaMaskDappBrowserUrl,
  hasInjectedEip1193,
  isAllowedMetaMaskDappLink,
  isEvmWalletConnectConnectorId,
  LEGAL_EVM_INAPP_HINT,
  shouldShowLegalEvmInAppHint,
} from '@/utils/legalEvmInAppHint'
import type { Network } from '@plasticdigits/cl8y-clickwrap'

/**
 * Next-step copy when Legal Accept still needs an in-app browser.
 * Terra: Keplr (DEX #554 / WC-M12). EVM: MetaMask + copy (#16).
 * Does not implement ADR-036 or portal EIP-191 (C1 / #5).
 */
export default function LegalKeplrInAppHint({
  address,
  network,
}: {
  address: string
  network: Network
}) {
  const connectorId = useEvmWalletStore((s) => s.connectorId)
  const [signedLatest, setSignedLatest] = useState<boolean | null>(null)

  useEffect(() => {
    if (network !== 'TerraClassic' && network !== 'EVM') {
      setSignedLatest(null)
      return
    }
    const apiNetwork = network === 'EVM' ? 'EVM' : 'TERRA_CLASSIC'
    let cancelled = false
    getLegalClickwrapClient()
      .getSignatureStatus(getLegalProperty(), apiNetwork, address)
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

  if (network === 'EVM') {
    return (
      <LegalEvmInAppHintBody
        address={address}
        connectorId={connectorId}
        signedLatest={signedLatest}
      />
    )
  }

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

function LegalEvmInAppHintBody({
  address,
  connectorId,
  signedLatest,
}: {
  address: string
  connectorId: string | null
  signedLatest: boolean | null
}) {
  const show = shouldShowLegalEvmInAppHint({
    hasInjectedEip1193: hasInjectedEip1193(),
    connectedViaWalletConnect: isEvmWalletConnectConnectorId(connectorId),
    signedLatest,
  })

  const signUrl = useMemo(() => {
    if (!show) return null
    return buildLegalEvmSignUrl({
      account: address,
      redirectUri: resolveLegalRedirectUri(),
    })
  }, [address, show])

  const metamaskHref = useMemo(() => {
    if (!signUrl) return null
    const href = buildMetaMaskDappBrowserUrl(signUrl)
    return href && isAllowedMetaMaskDappLink(href) ? href : null
  }, [signUrl])

  if (!show) return null

  return (
    <section
      data-testid="legal-evm-inapp-hint"
      className="app-connected-terms-keplr-hint app-connected-terms-evm-hint"
    >
      <p>{LEGAL_EVM_INAPP_HINT}</p>
      <div className="app-connected-terms-evm-actions">
        {metamaskHref ? (
          <a
            data-testid="legal-evm-open-metamask"
            className="btn-muted"
            href={metamaskHref}
          >
            Open in MetaMask
          </a>
        ) : null}
        {signUrl ? (
          <CopyButton
            text={signUrl}
            ariaLabel="Copy Legal terms link"
            buttonLabel="Copy link"
            data-testid="legal-evm-copy-link"
          />
        ) : null}
      </div>
    </section>
  )
}
