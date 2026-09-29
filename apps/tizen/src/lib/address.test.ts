import { describe, expect, it } from 'vitest';

import { isPrivateAddress, normaliseAddress } from './address';

function ok(raw: string): boolean {
  const base = normaliseAddress(raw);
  return base !== null && isPrivateAddress(base);
}

describe('normaliseAddress', () => {
  it('adds the scheme and the default port a person leaves out', () => {
    expect(normaliseAddress('192.168.1.20')).toBe('http://192.168.1.20:7331');
    expect(normaliseAddress(' 192.168.1.20:7332/ ')).toBe('http://192.168.1.20:7332');
    expect(normaliseAddress('[fd00::1]:7331')).toBe('http://[fd00::1]:7331');
  });

  it('refuses text that cannot be an address, and credentials smuggled into one', () => {
    expect(normaliseAddress('')).toBeNull();
    expect(normaliseAddress('http://')).toBeNull();
    expect(normaliseAddress('http://user:pw@192.168.1.2')).toBeNull();
  });
});

describe('isPrivateAddress', () => {
  it('accepts every private, link-local, loopback and CGNAT range, and .local names', () => {
    for (const a of [
      '10.0.0.1',
      '172.16.0.1',
      '172.31.255.255',
      '192.168.100.61:7332',
      '169.254.1.1',
      '127.0.0.1',
      '100.64.0.1',
      '100.127.255.254',
      'desk.local',
      '[fe80::1]',
      '[fd12::3]',
      '[fc00::1]',
      '[::1]',
    ]) {
      expect(ok(a), a).toBe(true);
    }
  });

  it('refuses public addresses that sit just outside each private range', () => {
    for (const a of [
      '8.8.8.8',
      '172.15.0.1',
      '172.32.0.1',
      '192.169.0.1',
      '100.63.0.1',
      '100.128.0.1',
      '11.0.0.1',
      // A domain that merely contains a private-looking prefix.
      '192.168.1.1.evil.example',
      'local.example.com',
      '[2001:db8::1]',
    ]) {
      expect(ok(a), a).toBe(false);
    }
  });

  it('judges the address the browser will actually connect to, not the spelling typed', () => {
    // WHATWG URL parsing canonicalises hex and integer forms, so `0x08080808`
    // is 8.8.8.8 and must be refused, while `2130706433` is 127.0.0.1.
    expect(ok('0x08080808')).toBe(false);
    expect(ok('2130706433')).toBe(true);
  });
});
