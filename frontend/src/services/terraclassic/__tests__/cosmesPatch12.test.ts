import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const frontendRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..')
const patchesDir = resolve(frontendRoot, 'patches')
const patchHashFile = resolve(patchesDir, '.cosmes-patch-sha256')

function cosmesLockfileVersion(): string {
  const lock = JSON.parse(readFileSync(resolve(frontendRoot, 'package-lock.json'), 'utf8')) as {
    packages?: Record<string, { version?: string }>
  }
  const version = lock.packages?.['node_modules/@goblinhunt/cosmes']?.version
  if (!version) {
    throw new Error('Could not resolve @goblinhunt/cosmes version from package-lock.json')
  }
  return version
}

function findCosmesPatchFile(): string {
  const version = cosmesLockfileVersion()
  const patchPath = resolve(patchesDir, `@goblinhunt+cosmes+${version}.patch`)
  if (!existsSync(patchPath)) {
    throw new Error(
      `No patch file for @goblinhunt/cosmes@${version} (expected patches/@goblinhunt+cosmes+${version}.patch)`,
    )
  }
  return patchPath
}

function sha256HexFile(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

function readCosmes(relPath: string): string {
  return readFileSync(resolve(frontendRoot, 'node_modules/@goblinhunt/cosmes', relPath), 'utf8')
}

describe('cosmes patch-package (voting #12 / DEX #519)', () => {
  it('patch file SHA-256 matches committed patches/.cosmes-patch-sha256', () => {
    const patchPath = findCosmesPatchFile()
    const actual = sha256HexFile(patchPath)
    const expected = readFileSync(patchHashFile, 'utf8').trim().split(/\s+/)[0]
    expect(actual).toBe(expected)
  })

  it('QRCodeModal delegates to __CL8Y_WC_PAIRING_MODAL__ and does not auto-redirect', () => {
    const src = readCosmes('dist/wallet/walletconnect/QRCodeModal.js')
    expect(src).toContain('__CL8Y_WC_PAIRING_MODAL__')
    expect(src).toContain('GitLab #519')
    expect(src).toContain('Copy pairing link')
    expect(src).toContain('do not auto-redirect')
    expect(src).not.toMatch(/if \(isMobile\(\)\) \{\s*\/\/ On mobile, redirect to mobile app/)
  })

  it('package.json postinstall runs patch-package', () => {
    const pkg = JSON.parse(readFileSync(resolve(frontendRoot, 'package.json'), 'utf8')) as {
      scripts?: Record<string, string>
    }
    expect(pkg.scripts?.postinstall).toContain('patch-package')
  })
})
