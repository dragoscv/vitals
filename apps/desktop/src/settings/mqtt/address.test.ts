import { describe, expect, it } from 'vitest';

import { checkBrokerAddress, type BrokerAddress } from './address';

/** Narrows an accepted check, failing loudly if the address was refused. */
function broker(raw: string): BrokerAddress {
  const check = checkBrokerAddress(raw);
  if (!check.ok) throw new Error(`expected ${raw} to be accepted, got ${check.problem}`);
  return check.broker;
}

describe('checkBrokerAddress', () => {
  it('accepts a plain address and leaves the port unset for the scheme default', () => {
    expect(broker('mqtt://broker.local')).toEqual({
      scheme: 'mqtt',
      host: 'broker.local',
      port: null,
    });
  });

  it('accepts an encrypted address and keeps the explicit port', () => {
    expect(broker('mqtts://192.168.1.5:8883')).toEqual({
      scheme: 'mqtts',
      host: '192.168.1.5',
      port: 8883,
    });
  });

  it('reads the host and port out of an IPv6 literal', () => {
    const parsed = broker('mqtt://[fe80::1]:1884');
    expect(parsed.port).toBe(1884);
    // The brackets are the URL syntax, not part of the address the client
    // dials; what matters here is that the host and port were split.
    expect(parsed.host).toContain('fe80::1');
  });

  it('ignores a trailing path or query, as the broker grammar does', () => {
    expect(broker('mqtt://broker.local:1883/').host).toBe('broker.local');
    expect(broker('mqtt://broker.local?x=1').host).toBe('broker.local');
  });

  it('accepts an upper-case scheme and surrounding whitespace as the same address', () => {
    expect(broker('  MQTT://broker.local  ').host).toBe('broker.local');
  });

  it('refuses a scheme that is not mqtt or mqtts', () => {
    expect(checkBrokerAddress('http://broker.local')).toEqual({ ok: false, problem: 'scheme' });
    expect(checkBrokerAddress('broker.local:1883')).toEqual({ ok: false, problem: 'scheme' });
  });

  it('refuses an address with no host however it was written', () => {
    for (const raw of ['mqtt://', 'MQTT:', 'mqtt:host', 'mqtt:///path']) {
      expect(checkBrokerAddress(raw).ok, raw).toBe(false);
    }
  });

  it('refuses a password smuggled into the address, which is stored in plain JSON', () => {
    expect(checkBrokerAddress('mqtt://alice:s3cret@broker.local')).toEqual({
      ok: false,
      problem: 'credentials',
    });
    // A bare username is refused too: the message sends both to the fields,
    // and an address with only a user name still is not what the field says.
    expect(checkBrokerAddress('mqtt://alice@broker.local')).toEqual({
      ok: false,
      problem: 'credentials',
    });
  });

  it('refuses a bad port rather than silently dropping it', () => {
    for (const raw of ['mqtt://broker.local:notaport', 'mqtt://broker.local:99999']) {
      expect(checkBrokerAddress(raw), raw).toEqual({ ok: false, problem: 'malformed' });
    }
  });

  it('treats an empty field as not-yet-set, not a mistake', () => {
    expect(checkBrokerAddress('')).toEqual({ ok: false, problem: 'empty' });
    expect(checkBrokerAddress('   ')).toEqual({ ok: false, problem: 'empty' });
  });
});
