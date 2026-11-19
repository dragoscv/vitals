/**
 * The broker address, parsed rather than pattern-matched.
 *
 * The address is a URL, so it is handed to the platform's URL parser and the
 * parts are read off the result. A regular expression would re-implement the
 * scheme, host and port rules badly, and the Rust side parses the same string
 * with its own grammar — agreeing with the standard is the safest way to
 * agree with it. A malformed address is refused here so the Settings screen
 * can explain it, rather than reaching the backend as a connection attempt
 * that fails for a reason nobody can see.
 */

export interface BrokerAddress {
  readonly scheme: 'mqtt' | 'mqtts';
  readonly host: string;
  /** `null` when no port is given and the scheme's default applies. */
  readonly port: number | null;
}

/**
 * Why an address was refused.
 *
 * A code rather than a sentence, so the panel picks the wording (and its
 * locale) — a message built here would be English only and would drift from
 * the translations the moment a reason was added.
 */
export type AddressProblem = 'empty' | 'scheme' | 'host' | 'credentials' | 'malformed';

export type AddressCheck =
  | { readonly ok: true; readonly broker: BrokerAddress }
  | { readonly ok: false; readonly problem: AddressProblem };

/**
 * Parses a broker address.
 *
 * `empty` is returned for a blank field rather than a `malformed` one: an
 * unfilled field is a section being set up, not a mistake, and the panel
 * treats it accordingly.
 *
 * Userinfo is refused on purpose. The address is written to the settings file
 * as plain JSON; the password is not — it goes to the OS secret store. A
 * password pasted inside the address would sidestep that, so it is rejected
 * with a message telling the user where it belongs.
 */
export function checkBrokerAddress(raw: string): AddressCheck {
  const text = raw.trim();
  if (text === '') return { ok: false, problem: 'empty' };

  let url: URL;
  try {
    url = new URL(text);
  } catch {
    // A WHATWG parse failure also covers a bad port: `mqtt://host:abc` and
    // `mqtt://host:99999` are both rejected by the parser, which is why the
    // message names the shape of a correct address instead of guessing which
    // part is wrong.
    return { ok: false, problem: 'malformed' };
  }

  // `protocol` keeps its trailing colon: `mqtt:`, `mqtts:`.
  if (url.protocol !== 'mqtt:' && url.protocol !== 'mqtts:') {
    return { ok: false, problem: 'scheme' };
  }
  if (url.username !== '' || url.password !== '') {
    return { ok: false, problem: 'credentials' };
  }
  // `mqtt://` and `mqtt:host` both parse with no host; the second names the
  // missing `//` as a missing host, which is the part a person can fix.
  if (url.hostname === '') return { ok: false, problem: 'host' };

  return {
    ok: true,
    broker: {
      scheme: url.protocol === 'mqtts:' ? 'mqtts' : 'mqtt',
      host: url.hostname,
      port: url.port === '' ? null : Number(url.port),
    },
  };
}
