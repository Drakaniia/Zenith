// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	// `site` (the public URL of the deployed docs) goes here once hosting is decided.
	// Starlight uses it for canonical URLs and the sitemap.
	// site: 'https://example.com',
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
			sidebar: [
				{ label: 'Getting started', items: [{ autogenerate: { directory: 'getting-started' } }] },
				{ label: 'Guides', items: [{ autogenerate: { directory: 'guides' } }] },
				{ label: 'Reference', items: [{ autogenerate: { directory: 'reference' } }] },
				{ label: 'Contributing', items: [{ autogenerate: { directory: 'contributing' } }] },
			],
		}),
	],
});
