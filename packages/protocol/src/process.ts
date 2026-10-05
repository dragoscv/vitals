/**
 * How a process is named for a person.
 *
 * One function so every surface — the desktop table, the detail panel, the
 * phone, the dashboard — agrees. The kernel only knows the image file name
 * (`Code - Insiders.exe`); the executable's own description ("Visual Studio
 * Code - Insiders") is what people recognise, and is what Task Manager shows.
 */

import type { Process } from './generated';

/** The app's own name when it declares one, else the file name. */
export function displayName(process: Pick<Process, 'name' | 'description'>): string {
  const description = process.description?.trim();
  return description === undefined || description === '' ? process.name : description;
}

/**
 * Whether `query` matches either name. Searching "code" must find VS Code,
 * and searching "explorer.exe" must still find Windows Explorer — people
 * arrive with either, the second often read out of a crash log.
 */
export function matchesProcessName(
  process: Pick<Process, 'name' | 'description'>,
  query: string,
): boolean {
  const needle = query.toLowerCase();
  return (
    process.name.toLowerCase().includes(needle) ||
    (process.description?.toLowerCase().includes(needle) ?? false)
  );
}
