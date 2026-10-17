// @ts-check
import { defineConfig, passthroughImageService } from 'astro/config';
import starlight from '@astrojs/starlight';

const SITE = 'https://vitals.dragoscatalin.ro';
const STORE = 'https://apps.microsoft.com/detail/9P95RK5VMGPJ';

// Structured data for search engines: what the product is, where to get it
// and that it is free. Every claim here is also on the page itself — Google
// treats markup that the visible page does not back up as spam.
const softwareApplication = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: 'Vitals',
  alternateName: 'Vitals: System Monitor',
  description:
    'A free, open-source system monitor and task manager for Windows, with a plain-language answer to "why is my PC slow?" and no telemetry.',
  operatingSystem: 'Windows 10, Windows 11',
  applicationCategory: 'UtilitiesApplication',
  url: `${SITE}/`,
  downloadUrl: STORE,
  installUrl: STORE,
  image: `${SITE}/og.png`,
  screenshot: `${SITE}/screenshots/dark-01-dashboard.png`,
  license: 'https://opensource.org/licenses/MIT',
  author: { '@type': 'Person', name: 'Dragos Catalin Vladulescu' },
  offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
};

export default defineConfig({
  // Served from a custom domain at the root, so no `base` is needed. Setting
  // one would break every absolute link and the CNAME mapping.
  site: SITE,
  trailingSlash: 'always',
  // Screenshots are pre-encoded WebP/PNG in public/ and the logo is an SVG,
  // so nothing gains from Sharp; the passthrough service keeps a native
  // image library out of the build.
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
      components: {
        Hero: './src/components/Hero.astro',
      },
      head: [
        {
          tag: 'script',
          attrs: { type: 'application/ld+json' },
          content: JSON.stringify(softwareApplication),
        },
        // PNG, not SVG: Facebook, LinkedIn, X and Slack do not render SVG previews.
        { tag: 'meta', attrs: { property: 'og:image', content: `${SITE}/og.png` } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        {
          tag: 'meta',
          attrs: {
            property: 'og:image:alt',
            content: 'Vitals — see what your computer is actually doing.',
          },
        },
        { tag: 'meta', attrs: { name: 'twitter:image', content: `${SITE}/og.png` } },
        { tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
        // `sizes` stops Chrome preferring the ICO over the SVG favicon.
        { tag: 'link', attrs: { rel: 'icon', href: '/favicon.ico', sizes: '32x32' } },
        { tag: 'link', attrs: { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' } },
        { tag: 'link', attrs: { rel: 'manifest', href: '/manifest.webmanifest' } },
        {
          tag: 'meta',
          attrs: { name: 'theme-color', media: '(prefers-color-scheme: dark)', content: '#0a1a10' },
        },
        {
          tag: 'meta',
          attrs: {
            name: 'theme-color',
            media: '(prefers-color-scheme: light)',
            content: '#f1f7f2',
          },
        },
      ],
      sidebar: [
        {
          label: 'Start here',
          translations: { ro: 'Începe aici' },
          items: ['download', 'guides/getting-started', 'faq'],
        },
        {
          label: 'Compare',
          translations: { ro: 'Comparații' },
          items: ['compare/task-manager', 'compare/process-explorer', 'compare/system-informer'],
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
