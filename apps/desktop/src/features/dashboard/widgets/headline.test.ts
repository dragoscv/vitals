import { describe, expect, it } from 'vitest';

import { makeSystem } from '../test-fixtures';
import { headlineFor } from './headline';

// The caption goes through i18n; its wording is not what these tests are about.
const t = ((key: string) => key) as never;

describe('headlineFor', () => {
  it('leads the CPU card with its total load and warns from 75 %', () => {
    const h = headlineFor('cpu', makeSystem({ cpu: { total: 80 } }), 'en', t);
    expect(h?.value).toMatch(/80/);
    expect(h?.tone).toBe('warn');
  });

  it('turns red at 90 % and stays neutral below 75 %', () => {
    expect(headlineFor('cpu', makeSystem({ cpu: { total: 95 } }), 'en', t)?.tone).toBe('danger');
    expect(headlineFor('cpu', makeSystem({ cpu: { total: 20 } }), 'en', t)?.tone).toBe('default');
  });

  it('gives no GPU headline when no adapter reports utilisation, rather than 0 %', () => {
    const system = { ...makeSystem(), gpus: [] };
    expect(headlineFor('gpu', system, 'en', t)).toBeNull();
  });

  it('has no headline for lists that no single number summarises', () => {
    expect(headlineFor('topCpu', makeSystem(), 'en', t)).toBeNull();
    expect(headlineFor('storage', makeSystem(), 'en', t)).toBeNull();
  });

  it('says nothing before the first frame', () => {
    expect(headlineFor('cpu', null, 'en', t)).toBeNull();
  });
});
