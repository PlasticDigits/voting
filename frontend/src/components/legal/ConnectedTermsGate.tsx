import type { ReactNode } from 'react'
import { TermsGate } from '@plasticdigits/cl8y-clickwrap/react'
import { useConnectedIdentity } from '@/hooks/useConnectedIdentity'
import LegalKeplrInAppHint from '@/components/legal/LegalKeplrInAppHint'
import {
  getLegalClickwrapClient,
  getLegalProperty,
  LEGAL_APP_NAME,
  resolveLegalRedirectUri,
  skipLegalClickwrapForAutomation,
} from '@/utils/legalClickwrap'

/**
 * Shell gate for wallet-bound CL8Y Legal acceptances.
 * Disconnected users keep browse access. Fail closed when status is unknown/error after connect.
 * Network is TerraClassic or EVM based on the connected chain.
 */
export default function ConnectedTermsGate({ children }: { children: ReactNode }) {
  const { address, legalNetwork } = useConnectedIdentity()

  if (skipLegalClickwrapForAutomation() || !address || !legalNetwork) {
    return <>{children}</>
  }

  const redirectUri = resolveLegalRedirectUri() ?? undefined
  const property = getLegalProperty()

  return (
    <div data-testid="connected-terms-gate" className="app-connected-terms-gate">
      <LegalKeplrInAppHint address={address} network={legalNetwork} />
      <TermsGate
        client={getLegalClickwrapClient()}
        property={property}
        network={legalNetwork}
        account={address}
        redirectUri={redirectUri}
        appName={LEGAL_APP_NAME}
        fallback={
          <div className="app-connected-terms-panel" role="status" aria-live="polite">
            <p className="app-connected-terms-lead">Checking terms acceptance…</p>
          </div>
        }
      >
        {children}
      </TermsGate>
    </div>
  )
}
