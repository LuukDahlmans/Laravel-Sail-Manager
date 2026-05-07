# Sail Manager website

Marketing site for [Sail Manager](https://github.com/LuukDahlmans/Laravel-Sail-Manager). Astro 5, no CSS framework — scoped component styles only.

Production: **https://sailmanager.app** (once DNS is configured).

## Develop

```sh
cd website
npm install
npm run dev      # http://localhost:4321
```

## Build

```sh
npm run build    # static output → ./dist
npm run preview
```

The output is fully static — drop `dist/` on any host (Vercel, Netlify, Cloudflare Pages, S3, GitHub Pages).

## Deployment (GitHub Pages)

The repo includes `.github/workflows/website.yml`, which auto-builds and deploys on every push to `main` that touches `website/**`. Two one-time setup steps before the first deploy goes live:

### 1. Enable GitHub Pages on the repo

Repo → **Settings → Pages**:

- **Source:** *GitHub Actions* (not "Deploy from a branch")
- Save.

That's it — the next push to `main` triggers the workflow and deploys.

### 2. Point the custom domain (optional but recommended)

`./public/CNAME` contains `sailmanager.app`, so the workflow already tells Pages to serve there. To make DNS work, add records at your domain registrar:

**For an apex domain (sailmanager.app, no www):**

| Type | Host | Value |
|---|---|---|
| A | @ | 185.199.108.153 |
| A | @ | 185.199.109.153 |
| A | @ | 185.199.110.153 |
| A | @ | 185.199.111.153 |

**Optionally also www → apex:**

| Type | Host | Value |
|---|---|---|
| CNAME | www | luukdahlmans.github.io |

After DNS propagates (usually 5–30 minutes), GitHub Pages will validate and start serving. The Pages settings page shows the verification status.

Until DNS is configured, the site is reachable at `https://luukdahlmans.github.io/Laravel-Sail-Manager/`. Internal links assume root-level deploy though, so they'll 404 on the `.github.io` URL. Configuring the custom domain is the path of least resistance.

### 3. Trigger the first deploy

Either push a commit that touches `website/**`, or manually run the workflow from the Actions tab → "Deploy website" → "Run workflow".

## Files

```
website/
├── .gitignore
├── astro.config.mjs        site URL, sitemap integration
├── package.json            astro 5 + @astrojs/sitemap
├── public/
│   ├── CNAME               custom domain (sailmanager.app)
│   ├── favicon.svg         waves-in-red-square logo
│   └── robots.txt          allow-all + sitemap pointer
├── src/
│   ├── content.config.ts   blog collection schema
│   ├── content/blog/       *.md posts
│   ├── layouts/            Layout.astro, DocsLayout.astro
│   ├── pages/              index.astro, blog/, docs/, 404.astro
│   ├── components/         Nav, Hero, Demo, Features, Workflow, Download, Faq, Footer, Logo, DemoIntro
│   ├── lib/docs.ts         single source of truth for docs nav + search
│   └── styles/global.css   shared tokens + utilities
└── README.md
```

## Adding a blog post

Drop a markdown file in `src/content/blog/your-slug.md`:

```yaml
---
title: 'Your post title'
description: 'One-sentence description for SEO + listings.'
publishedAt: 2026-05-08
tags: ['laravel-sail', 'tutorial']
keywords: ['primary keyword', 'long-tail variant']
---

Your post body in markdown. Inline HTML is supported, including the
<span class="recommended">Recommended</span> badge for ranked-options posts.
```

Astro auto-generates the page at `/blog/your-slug`, adds it to the listing, includes it in the sitemap, and emits Article JSON-LD for SEO.

## Adding a docs page

1. Drop `src/pages/docs/your-topic.astro` wrapped in `<DocsLayout slug="your-topic" title="..." description="...">`.
2. Register it in `src/lib/docs.ts` under the appropriate group with its sections list.

The sidebar nav, the landing-page cards, the search index, and the prev/next navigation all read from `src/lib/docs.ts` — adding a page there is enough.

## SEO baked in

- Single-page semantic HTML (header / main / section / footer).
- Per-page title + description, canonical URL, Open Graph + Twitter cards.
- JSON-LD `SoftwareApplication` schema on home; `FAQPage` schema on the FAQ; `Article` schema on every blog post.
- `@astrojs/sitemap` generates `sitemap-index.xml` automatically.
- `robots.txt` allows everything and points at the sitemap.
- Fast first paint — Astro ships zero JS by default; only the docs search and demo interactivity carry small inline scripts.
- All images have `alt` text; interactive icons are `aria-hidden`.
- Skip-link at the top for keyboard users.
- `prefers-reduced-motion` respected on every animation.
