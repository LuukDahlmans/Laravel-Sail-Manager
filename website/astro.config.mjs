import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

// Update SITE_URL once the production domain is known. Affects sitemap,
// canonical URLs, and Open Graph absolute paths.
const SITE_URL = 'https://sailmanager.app';

export default defineConfig({
  site: SITE_URL,
  integrations: [sitemap()],
  build: {
    inlineStylesheets: 'auto',
  },
  compressHTML: true,
});
