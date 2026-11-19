/**
 * The MQTT settings' round trip.
 *
 * The settings file is plain JSON a user can edit, so what it stores and what
 * it reads back have to be the same shape — and the shape has to be the
 * non-secret half only. These tests exist because a password reaching that
 * file is the one mistake the section must not make.
 */

import { describe, expect, it } from 'vitest';

import { defaultSettings, parseSettings } from '../schema';

describe('the MQTT settings', () => {
  it('are off, with no broker, out of the box', () => {
    expect(defaultSettings.mqtt).toEqual({ enabled: false, url: '', username: '' });
    expect(parseSettings({}).mqtt).toEqual(defaultSettings.mqtt);
  });

  it('survive a round trip through JSON unchanged', () => {
    const mqtt = {
      enabled: true,
      url: 'mqtt://homeassistant.local:1883',
      username: 'vitals',
    };
    const stored: unknown = JSON.parse(JSON.stringify({ ...defaultSettings, mqtt }));
    expect(parseSettings(stored).mqtt).toEqual(mqtt);
  });

  it('keeps a good field when a neighbouring one is the wrong type', () => {
    // One bad value must cost that field, not the whole section.
    const parsed = parseSettings({
      mqtt: { enabled: 'yes', url: 'mqtt://broker.local', username: 7 },
    });
    expect(parsed.mqtt).toEqual({ enabled: false, url: 'mqtt://broker.local', username: '' });
  });

  it('drops a password left in the file, because the secret never lives there', () => {
    // The password is kept in the OS secret store. A `password` key in the
    // settings file is not part of the shape and must not be carried through
    // as if it were.
    const parsed = parseSettings({
      mqtt: { enabled: true, url: 'mqtt://broker.local', username: '', password: 's3cret' },
    });
    expect(parsed.mqtt).toEqual({ enabled: true, url: 'mqtt://broker.local', username: '' });
    expect(parsed.mqtt).not.toHaveProperty('password');
  });

  it('falls back to the default when the stored value is not an object', () => {
    expect(parseSettings({ mqtt: 'mqtt://broker.local' }).mqtt).toEqual(defaultSettings.mqtt);
    expect(parseSettings({ mqtt: null }).mqtt).toEqual(defaultSettings.mqtt);
  });
});
