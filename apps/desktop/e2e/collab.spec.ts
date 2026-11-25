import { expect, test } from '@playwright/test';

import { collabData, installFakeHost } from './fakeHost';
import { expectNoProblems, openRoute, visibleRoute, watchProblems } from './support';

/**
 * The collaboration page with a coordination directory to read.
 *
 * The per-route specs in navigation/layout/a11y run against the no-host
 * fallback, where this screen is an empty state and nothing below the title
 * is exercised. Here a fake host answers `get_collab`, so the three sections,
 * the em dashes and the ages are rendered and measured in a real browser —
 * which is the only way a layout defect in them shows up.
 */

test('shows who is here, what they are saying, and what is still running', async ({ page }) => {
  const problems = watchProblems(page);
  await installFakeHost(page, { locale: 'en', collab: collabData() });
  await openRoute(page, 'collab');

  const screen = visibleRoute(page);
  await expect(
    screen.getByRole('heading', { level: 2, name: /^Agent collaboration/ }),
  ).toBeVisible();

  // 1. Agents present, live first, with the reason it counts as present.
  const agents = screen.getByRole('region', { name: 'Agents present' });
  await expect(agents.getByText('2 agents')).toBeVisible();
  await expect(agents.getByText('1 present · a call in the last 30 min')).toBeVisible();
  await expect(agents.getByText('3f2a91c4')).toBeVisible();
  await expect(agents.getByText('b7e0d155')).toBeVisible();
  await expect(agents.getByText('Doing: the gate on the invoice slice')).toBeVisible();

  // 2. Work in progress, with a running job and a finished one beside it.
  const work = screen.getByRole('region', { name: 'Work in progress' });
  await expect(work.getByText('2 jobs')).toBeVisible();
  await expect(work.getByText('1 running')).toBeVisible();
  // Exact: "Running" also matches the "1 running" count line above it.
  await expect(work.getByText('Running', { exact: true })).toBeVisible();
  await expect(work.getByText('Done', { exact: true })).toBeVisible();

  // 3. Conversations, with the reply nested under the question it answers.
  const threads = screen.getByRole('region', { name: 'Conversations' });
  await expect(threads.getByText('Answered')).toBeVisible();
  await expect(
    threads.getByText(
      'Holding the build ticket until the migration lands. Ping me when it is safe.',
    ),
  ).toBeVisible();
  await expect(threads.getByText('Landed. The ticket is yours.')).toBeVisible();

  expectNoProblems(problems);
});

test('reads a coordination directory that is not there as an empty page, not as a failure', async ({
  page,
}) => {
  const problems = watchProblems(page);
  await installFakeHost(page, {
    locale: 'en',
    collab: {
      takenAtMs: Date.now(),
      installed: false,
      livenessMin: 30,
      agents: [],
      threads: [],
      jobs: [],
    },
  });
  await openRoute(page, 'collab');

  const screen = visibleRoute(page);
  await expect(
    screen.getByRole('heading', { level: 2, name: /^Agent collaboration/ }),
  ).toBeVisible();
  // Says why it is empty, and does not dress an absent queue up as a zero one.
  await expect(
    screen.getByText('agentq keeps no coordination state on this machine yet', {
      exact: false,
    }),
  ).toBeVisible();
  await expect(screen.getByText('No agent has messaged another yet.')).toBeVisible();
  await expect(screen.getByText('1 agent', { exact: false })).toHaveCount(0);
  expectNoProblems(problems);
});
