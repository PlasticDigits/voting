import { describe, expect, it } from 'vitest'
import { buildSignUrl } from '../cl8y-clickwrap'

describe('vendor buildSignUrl (issue #16)', () => {
  it('passes property, redirect_uri, app_name, and account', () => {
    const href = buildSignUrl('https://terms.cl8y.com/sign/evm', {
      property: 'vote.cl8y.com',
      redirectUri: 'https://vote.cl8y.com/vote',
      appName: 'CL8Y Voting',
      account: '0x1111111111111111111111111111111111111111',
    })
    const url = new URL(href)
    expect(url.pathname).toBe('/sign/evm')
    expect(url.searchParams.get('property')).toBe('vote.cl8y.com')
    expect(url.searchParams.get('redirect_uri')).toBe('https://vote.cl8y.com/vote')
    expect(url.searchParams.get('app_name')).toBe('CL8Y Voting')
    expect(url.searchParams.get('account')).toBe('0x1111111111111111111111111111111111111111')
  })

  it('omits empty account and never treats account as the href', () => {
    const href = buildSignUrl('https://terms.cl8y.com/sign/evm', {
      account: '   ',
      redirectUri: 'https://vote.cl8y.com/',
    })
    expect(new URL(href).searchParams.has('account')).toBe(false)
    expect(href.startsWith('https://terms.cl8y.com/sign/evm')).toBe(true)
  })
})
