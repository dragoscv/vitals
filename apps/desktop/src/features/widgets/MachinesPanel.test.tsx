/**
 * The machines panel's guarantees.
 *
 * This panel is the only place a widget can be pointed at another computer, so
 * what is asserted here is the part that is expensive to get wrong rather than
 * the part that is easy to look at: the list reaches the data layer in the shape
 * the remote client expects, an address Windows would hand to the wrong service
 * is refused before anything is registered, and a pairing token is never shown
 * whole. The last one is not a nicety — a full token on screen is a credential
 * on a screenshot.
 *
 * The registry is a module singleton, so it is mocked rather than driven: the
 * assertion is the exact list the panel publishes, which a real registry would
 * only let us observe by reading it back through the same module.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import {
  MachinesPanel,
  parseAddress,
  tokenPrefix,
  useMachines,
  type MachineEntry,
} from './MachinesPanel';
import { setRemoteConnections } from './remote';

vi.mock('./remote', async (importOriginal) => {
  // The real module, with only the registry replaced. The generic is the shape
  // being spread; naming it here avoids restating the module's exported types
  // and a cast that would say the same thing less clearly.
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, setRemoteConnections: vi.fn() };
});

beforeAll(async () => {
  await initI18n('en');
  // The copy this panel needs is not in the shipped locale files yet — it is
  // reported for a translator rather than invented here. The key that
  // interpolates has to exist for the prefix to reach the screen at all, since
  // a missing key is returned verbatim with its placeholders intact, and then
  // "the token is never shown whole" would be true for the wrong reason.
  i18n.addResourceBundle(
    'en',
    'translation',
    { widgets: { machines: { key: 'Key {{prefix}}' } } },
    true,
    true,
  );
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  vi.mocked(setRemoteConnections).mockClear();
});

const registry = (): readonly unknown[] =>
  vi.mocked(setRemoteConnections).mock.calls.at(-1)?.[0] ?? [];

/** The panel wired to the real `useMachines`, which is what writes the registry. */
function Panel({
  seed,
}: {
  readonly seed?: readonly MachineEntry[];
} = {}): React.JSX.Element {
  const machines = useMachines({
    ...(seed !== undefined && {
      backend: {
        load: async () => seed,
        save: async () => {},
      },
    }),
  });
  return (
    <MachinesPanel
      machines={machines.machines}
      hydrated={machines.hydrated}
      onAdd={machines.add}
      onRemove={machines.remove}
    />
  );
}

/** The add form's three fields, by the labels the user reads. */
function fields(): { name: HTMLInputElement; address: HTMLInputElement; token: HTMLInputElement } {
  return {
    name: screen.getByLabelText(i18n.t('widgets.machines.name')),
    address: screen.getByLabelText(i18n.t('widgets.machines.address')),
    token: screen.getByLabelText(i18n.t('widgets.machines.token')),
  };
}

/** The form's submit button, which stays dead until the stored list has been read. */
function addButton(): HTMLButtonElement {
  return screen.getByRole<HTMLButtonElement>('button', { name: i18n.t('widgets.machines.add') });
}

/** Fills the form and submits it the way a form submits, not by clicking. */
function submit(label: string, address: string, token: string): void {
  const { name, address: addressField, token: tokenField } = fields();
  fireEvent.change(name, { target: { value: label } });
  fireEvent.change(addressField, { target: { value: address } });
  fireEvent.change(tokenField, { target: { value: token } });
  // jsdom does not run a form's default submission from a button click, so the
  // submit event is what has to be fired for `onSubmit` to run at all.
  const form = addButton().closest('form');
  if (form === null) throw new Error('the add button is not in a form');
  fireEvent.submit(form);
}

describe('the machines panel', () => {
  it('registers the machine the user added, in the shape the remote client reads', async () => {
    render(<Panel />);

    await waitFor(() => {
      expect(addButton().disabled).toBe(false);
    });

    submit('Rig', 'http://192.168.1.20:7331', 's3cret-token');

    await waitFor(() => {
      expect(registry()).toEqual([
        {
          id: expect.any(String),
          label: 'Rig',
          baseUrl: 'http://192.168.1.20:7331',
          token: 's3cret-token',
        },
      ]);
    });
    // The address is stored as typed rather than normalised into something else:
    // what the card displays is what the poll dials.
    expect(screen.getByText('http://192.168.1.20:7331')).toBeDefined();
    expect(fields().name.value).toBe('');
  });

  it('drops a machine from the registry when it is forgotten', async () => {
    render(
      <Panel
        seed={[{ id: 'rig', label: 'Rig', baseUrl: 'http://rig:7331', token: 's3cret-token' }]}
      />,
    );

    const forget = await waitFor(() =>
      screen.getByRole('button', { name: i18n.t('widgets.machines.remove', { name: 'Rig' }) }),
    );
    // Wait for the seeded list to reach the registry before removing it, so the
    // assertion below is about the removal and not about the initial publish.
    await waitFor(() => {
      expect(registry()).toHaveLength(1);
    });

    fireEvent.click(forget);

    await waitFor(() => {
      expect(registry()).toEqual([]);
    });
  });

  it('refuses an address that is not an http machine, without registering anything', async () => {
    render(<Panel />);

    await waitFor(() => {
      expect(addButton().disabled).toBe(false);
    });

    // `ftp:` would be dialled by the HTTP client as a nonsense URL, and a bare
    // host has no scheme at all: both fail on the far side as an unreachable
    // machine, which reads to the user as "the other PC is off".
    for (const address of ['rig', 'ftp://rig:7331', '   ']) {
      submit('Rig', address, 's3cret-token');

      await waitFor(() => {
        expect(screen.getByText(i18n.t('widgets.machines.error.address'))).toBeDefined();
      });
      expect(registry()).toEqual([]);
    }
  });

  it('refuses a machine with no name and one with no key', async () => {
    render(<Panel />);

    await waitFor(() => {
      expect(addButton().disabled).toBe(false);
    });

    submit('', 'http://rig:7331', 's3cret-token');
    await waitFor(() => {
      expect(screen.getByText(i18n.t('widgets.machines.error.name'))).toBeDefined();
    });
    expect(registry()).toEqual([]);

    submit('Rig', 'http://rig:7331', '');
    await waitFor(() => {
      expect(screen.getByText(i18n.t('widgets.machines.error.token'))).toBeDefined();
    });
    expect(registry()).toEqual([]);
  });

  it('shows a key as a prefix, never whole', async () => {
    render(
      <Panel
        seed={[{ id: 'rig', label: 'Rig', baseUrl: 'http://rig:7331', token: 's3cret-token' }]}
      />,
    );

    const prefix = tokenPrefix('s3cret-token');
    await waitFor(() => {
      // The whole line, because the prefix is interpolated into the label
      // rather than left as a node of its own.
      expect(screen.getByText(`Key ${prefix}`)).toBeDefined();
    });

    // The panel and the whole document: a token in a toolbar or a tooltip is
    // just as exposed as one in the row.
    expect(document.body.textContent).not.toContain('s3cret-token');
    // And the prefix is genuinely shorter than the secret, so this cannot pass
    // by the two strings being equal.
    expect(prefix.length).toBeLessThan('s3cret-token'.length);
  });
});

describe('the address parser', () => {
  it('accepts an http or https address and keeps it exactly as typed', () => {
    expect(parseAddress('  http://192.168.1.20:7331  ')).toBe('http://192.168.1.20:7331');
    expect(parseAddress('https://rig.local')).toBe('https://rig.local');
  });

  it('refuses anything it cannot dial as a machine', () => {
    for (const value of ['', '   ', 'rig', 'rig:7331', 'ftp://rig', 'file:///etc/passwd']) {
      expect(parseAddress(value)).toBeNull();
    }
  });
});
