import { devices, expect, test } from '@playwright/test';

import { expectNoProblems, watchProblems } from './support';

// A phone, because that is the only thing that ever loads this page: the LAN
// server hands it to whatever scanned the pairing QR code.
// Top level, not inside a describe: the device carries `defaultBrowserType`,
// which forces a new worker and is refused inside a group.
test.use({ ...devices['Pixel 7'] });

test('the phone page with nothing paired explains how to pair, with no console or page errors', async ({
  page,
}) => {
  const problems = watchProblems(page);
  await page.goto('/mobile.html');

  // A fresh browser has no pairings in localStorage and no token in the URL
  // fragment, so the only honest screen is the empty state telling the user
  // where to find the QR code. `role="status"` comes from EmptyState.
  const empty = page.getByRole('status').filter({ hasText: 'No PC is paired yet' });
  await expect(empty).toBeVisible();
  await expect(empty).toContainText('Remote access');
  // Not the failure variant: no pairing was attempted, so claiming one
  // failed would send the user chasing a problem that does not exist.
  await expect(page.getByText('That pairing did not work')).toHaveCount(0);

  expectNoProblems(problems);
});
