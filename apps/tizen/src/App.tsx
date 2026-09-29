/**
 * The shell: a navigation rail on the left that stays on screen, the current
 * screen to its right, and one key handler for the whole app.
 *
 * The rail is always visible rather than a drawer that slides out: Left from
 * the leftmost control reaches it with the ordinary spatial rule, and it
 * never covers a reading.
 */

import { useEffect, useLayoutEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';

import { PairingStore } from './lib/pairing';
import { AddScreen } from './screens/AddScreen';
import { OverviewScreen } from './screens/OverviewScreen';
import { PcListScreen } from './screens/PcListScreen';
import { SettingsScreen } from './screens/SettingsScreen';
import { TvScreen } from './screens/TvScreen';
import { PcScreen } from './screens/pc/PcScreen';
import { AppContext, useNavigatorState, type TopRoute } from './ui/app-context';
import { exitApp, focusFirst, isBackKey, moveFocus } from './ui/focus';

const RAIL: readonly { route: TopRoute; label: string; icon: string }[] = [
  { route: 'overview', label: 'nav.overview', icon: '⌂' },
  { route: 'tv', label: 'nav.tv', icon: '▭' },
  { route: 'pcs', label: 'nav.pcs', icon: '▣' },
  { route: 'add', label: 'nav.add', icon: '+' },
  { route: 'settings', label: 'nav.settings', icon: '⚙' },
];

export function App() {
  const { t } = useTranslation();
  const nav = useNavigatorState();
  const pairings = useMemo(() => new PairingStore(localStorage), []);
  const content = useRef<HTMLElement>(null);
  const services = useMemo(() => ({ nav, pairings }), [nav, pairings]);
  const current = nav.current;
  const screenKey = current.name === 'pc' ? `pc:${current.id}` : current.name;

  // A new screen means "go there": hand focus to it, as every TV app does.
  // Layout effect, then one frame later for screens that render their first
  // focusable after data arrives.
  useLayoutEffect(() => {
    if (!focusFirst(content.current)) {
      const id = requestAnimationFrame(() => focusFirst(content.current));
      return () => cancelAnimationFrame(id);
    }
    return undefined;
  }, [screenKey]);

  const navRef = useRef(nav);
  useEffect(() => {
    navRef.current = nav;
  });
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isBackKey(event)) {
        event.preventDefault();
        if (!navRef.current.pop()) exitApp();
        return;
      }
      if (moveFocus(event)) event.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const active = current.name === 'pc' ? 'pcs' : current.name;
  return (
    <AppContext.Provider value={services}>
      <div className="shell">
        <nav className="rail" aria-label="Vitals" data-zone="rail">
          <div className="brand" aria-hidden="true">
            <img src="./icon.png" alt="" />
          </div>
          {RAIL.map((item) => (
            <button
              type="button"
              key={item.route}
              className={`rail-item ${active === item.route ? 'is-selected' : ''}`}
              aria-current={active === item.route ? 'page' : undefined}
              onClick={() => nav.top(item.route)}
            >
              <span className="rail-icon" aria-hidden="true">
                {item.icon}
              </span>
              <span className="rail-label">{t(item.label)}</span>
            </button>
          ))}
        </nav>
        <main className="content" ref={content} key={screenKey} data-zone="content">
          {current.name === 'overview' && <OverviewScreen />}
          {current.name === 'tv' && <TvScreen />}
          {current.name === 'pcs' && <PcListScreen />}
          {current.name === 'add' && <AddScreen />}
          {current.name === 'settings' && <SettingsScreen />}
          {current.name === 'pc' && <PcScreen id={current.id} />}
        </main>
      </div>
    </AppContext.Provider>
  );
}
