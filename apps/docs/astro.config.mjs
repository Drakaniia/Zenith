// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Hosted on GitHub Pages as a project site (.github/workflows/docs.yml), so every page lives
// under /Zenith/. Set DOCS_SITE / DOCS_BASE to host it elsewhere (a custom domain: base "/").
const site = process.env.DOCS_SITE ?? 'https://drakaniia.github.io';
const base = process.env.DOCS_BASE ?? '/Zenith';

// Links between pages in content are written relative (`[Glance](../glance/)`), because Astro
// doesn't add `base` to links written in Markdown.

export default defineConfig({
	site,
	base,
	trailingSlash: 'always',
	integrations: [
		starlight({
			title: 'Zenith',
			description:
				'Documentation for Zenith, a lightweight Zen-style web browser for Windows built on WebView2.',
			// Shared with the desktop app through the workspace package.
			logo: { src: '@zenith/brand/logo.svg', alt: 'Zenith' },
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/Drakaniia/Zenith' },
			],
			editLink: { baseUrl: 'https://github.com/Drakaniia/Zenith/edit/main/apps/docs/' },
			sidebar: [
				{ label: 'Getting started', items: [{ autogenerate: { directory: 'getting-started' } }] },
				{ label: 'Guides', items: [{ autogenerate: { directory: 'guides' } }] },
				{ label: 'Reference', items: [{ autogenerate: { directory: 'reference' } }] },
				{ label: 'Contributing', items: [{ autogenerate: { directory: 'contributing' } }] },
			],
		}),
	],
});
