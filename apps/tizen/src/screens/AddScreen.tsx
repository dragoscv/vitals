/**
 * Adding a PC: its address, then the six-digit code it shows. The code can
 * be typed with the remote's number keys (registered in `focus.ts`) or with
 * the on-screen keypad; the token route is a fallback for a PC whose Vitals
 * predates pairing codes.
 *
 * No discovery list: finding PCs needs mDNS, which is UDP multicast, and a
 * TV web app has no UDP. The address is on the PC's Remote access page.
 */

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { normaliseAddress } from '../lib/address';
import { cleanCode, isCode, pairWithCode, pairWithToken, type PairRefusal } from '../lib/pairing';
import { useApp, usePairings } from '../ui/app-context';
import { Action, Hint, ScreenTitle } from '../ui/components';
import { digitOf, focusFirst } from '../ui/focus';

type Step = 'address' | 'code' | 'token';

const KEYPAD = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'] as const;

export function AddScreen() {
  const { t } = useTranslation();
  const { nav, pairings: store } = useApp();
  const pairings = usePairings();
  const [step, setStep] = useState<Step>('address');
  const [address, setAddress] = useState('');
  const [code, setCode] = useState('');
  const [token, setToken] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ reason: PairRefusal; status?: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);

  // Each step starts with focus on its first control, as a TV user expects.
  useEffect(() => {
    focusFirst(root.current);
  }, [step]);

  const submitCode = async (digits: string) => {
    if (busy || !isCode(digits)) return;
    setBusy(true);
    setError(null);
    const result = await pairWithCode(
      address,
      digits,
      t('add.tvLabel'),
      pairings,
      fetch.bind(globalThis),
    );
    setBusy(false);
    if (result.ok) {
      store.upsert(result.pairing);
      nav.top('overview');
      return;
    }
    setError(
      result.status === undefined
        ? { reason: result.reason }
        : { reason: result.reason, status: result.status },
    );
    if (result.reason === 'badCode') setCode('');
  };

  const submitToken = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    const result = await pairWithToken(address, token, pairings, fetch.bind(globalThis));
    setBusy(false);
    if (result.ok) {
      store.upsert(result.pairing);
      nav.top('overview');
      return;
    }
    setError(
      result.status === undefined
        ? { reason: result.reason }
        : { reason: result.reason, status: result.status },
    );
  };

  const type = (digit: string) => {
    if (busy) return;
    setError(null);
    const next = cleanCode(code + digit).slice(0, 6);
    setCode(next);
    // Six digits typed is the user saying "go": no extra OK to find.
    if (next.length === 6) void submitCode(next);
  };

  // The remote's number keys, while the code step is showing.
  useEffect(() => {
    if (step !== 'code') return;
    const onKey = (event: KeyboardEvent) => {
      const digit = digitOf(event);
      if (digit === null) return;
      event.preventDefault();
      type(digit);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const goToCode = () => {
    const base = normaliseAddress(address);
    if (base === null) {
      setError({ reason: 'badAddress' });
      return;
    }
    setError(null);
    setStep('code');
  };

  const shown = normaliseAddress(address)?.replace(/^https?:\/\//, '') ?? address;
  return (
    <div className="screen" ref={root}>
      <ScreenTitle>{t('add.title')}</ScreenTitle>
      <Hint>{t('add.how')}</Hint>

      {step === 'address' && (
        <div className="stack form">
          <label className="field">
            <span>{t('add.address')}</span>
            <input
              type="text"
              inputMode="url"
              autoComplete="off"
              spellCheck={false}
              placeholder={t('add.addressHint')}
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') goToCode();
              }}
              data-autofocus=""
            />
          </label>
          <Hint>{t('add.addressBody')}</Hint>
          <div className="choice-row">
            <Action label={t('add.next')} onPress={goToCode} />
            <Action
              label={t('add.useToken')}
              onPress={() => {
                setError(null);
                setStep('token');
              }}
            />
          </div>
        </div>
      )}

      {step === 'code' && (
        <div className="stack form">
          <h2>{t('add.codeTitle', { pc: shown })}</h2>
          <Hint>{t('add.codeBody')}</Hint>
          <div className="code" aria-label={t('add.codeTitle', { pc: shown })} aria-live="polite">
            {Array.from({ length: 6 }, (_, i) => (
              <span key={i} className={`code-cell ${i === code.length ? 'is-next' : ''}`}>
                {code[i] ?? ''}
              </span>
            ))}
          </div>
          <div className="keypad">
            {KEYPAD.map((d) => (
              <button
                type="button"
                key={d}
                className="key"
                disabled={busy}
                onClick={() => type(d)}
                {...(d === '1' && { 'data-autofocus': '' })}
              >
                {d}
              </button>
            ))}
            <button
              type="button"
              className="key key-wide"
              disabled={busy}
              onClick={() => setCode((c) => c.slice(0, -1))}
            >
              {t('add.codeDelete')}
            </button>
          </div>
          <div className="choice-row">
            <Action label={t('add.change')} onPress={() => setStep('address')} />
            <Action label={t('add.useToken')} onPress={() => setStep('token')} />
          </div>
        </div>
      )}

      {step === 'token' && (
        <div className="stack form">
          <label className="field">
            <span>{t('add.address')}</span>
            <input
              type="text"
              autoComplete="off"
              spellCheck={false}
              placeholder={t('add.addressHint')}
              value={address}
              onChange={(e) => setAddress(e.target.value)}
            />
          </label>
          <label className="field">
            <span>{t('add.token')}</span>
            <input
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void submitToken();
              }}
              data-autofocus=""
            />
          </label>
          <Hint>{t('add.tokenBody')}</Hint>
          <div className="choice-row">
            <Action label={t('add.pair')} disabled={busy} onPress={() => void submitToken()} />
            <Action label={t('add.useCode')} onPress={() => setStep('address')} />
          </div>
        </div>
      )}

      {busy && <Hint>{t('pairing.checking')}</Hint>}
      {error !== null && (
        <p className="outcome tone-danger" role="alert">
          {t(`pairing.${error.reason}`, { status: error.status ?? 0 })}
        </p>
      )}
    </div>
  );
}
