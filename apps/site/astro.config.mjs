// @ts-check
import { defineConfig, passthroughImageService } from 'astro/config';
import starlight from '@astrojs/starlight';

const SITE = 'https://vitals.dragoscatalin.ro';

export default defineConfig({
  // Served from a custom domain at the root, so no `base` is needed. Setting
  // one would break every absolute link and the CNAME mapping.
  site: SITE,
  trailingSlash: 'always',
  // Every image on the site is an SVG, which gains nothing from Sharp; the
  // passthrough service keeps a native image library out of the build.
  image: { service: passthroughImageService() },
  integrations: [
    starlight({
      title: 'Vitals',
      // Starlight renders this as <meta name="description"> on every page
      // without its own description, so it is not repeated in `head`.
      description:
        'Vitals is a free, open-source system monitor and task manager for Windows. Honest numbers, a plain-language answer to "why is my PC slow?", and no telemetry.',
      logo: {
        src: './src/assets/logo.svg',
        // Decorative: the site title is rendered as text right beside it, so a
        // non-empty alt would make screen readers announce "Vitals Vitals".
        alt: '',
        replacesTitle: false,
      },
      favicon: '/favicon.svg',
      defaultLocale: 'root',
      locales: {
        root: { label: 'English', lang: 'en' },
        ro: { label: 'Română', lang: 'ro' },
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/dragoscv/vitals',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/dragoscv/vitals/edit/main/apps/site/',
      },
      lastUpdated: false,
      customCss: ['./src/styles/theme.css'],
      head: [
        { tag: 'meta', attrs: { property: 'og:image', content: `${SITE}/og.svg` } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        {
          tag: 'meta',
          attrs: {
            property: 'og:image:alt',
            content: 'Vitals — see what your computer is actually doing.',
          },
        },
        { tag: 'meta', attrs: { name: 'twitter:image', content: `${SITE}/og.svg` } },
        { tag: 'meta', attrs: { name: 'theme-color', content: '#0b1416' } },
      ],
      sidebar: [
        {
          label: 'Start here',
          translations: { ro: 'Începe aici' },
          items: ['download', 'guides/getting-started'],
        },
        {
          label: 'Guides',
          translations: { ro: 'Ghiduri' },
          items: [
            'guides/remote-access',
            'guides/cli',
            'guides/integrations',
            'guides/troubleshooting',
          ],
        },
        {
          label: 'Reference',
          translations: { ro: 'Referință' },
          items: ['reference/architecture'],
        },
        {
          label: 'Community',
          translations: { ro: 'Comunitate' },
          items: ['community/contributing', 'community/changelog'],
        },
        {
          label: 'Legal',
          translations: { ro: 'Juridic' },
          items: ['privacy', 'terms'],
        },
      ],
    }),
  ],
});
