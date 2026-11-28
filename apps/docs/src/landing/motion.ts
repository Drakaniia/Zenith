// Landing page motion, timed like zen-browser.app (which uses anime.js v4): each section's targets
// fade up out of a blur, 300 ms apiece, staggered 150 ms in document order, once the section enters
// the viewport. Written with the Web Animations API so the docs site needs no animation library.

// zen-browser.app declares cubicBezier(0.25, 0.1, 0.25, 1), but anime.js doesn't parse that string
// and the reveal plays linearly; measured frame by frame, so match what it does.
const EASE = 'linear';
const OUT_QUAD = 'cubic-bezier(0.5, 1, 0.89, 1)'; // anime.js default ease, out(2)
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

function reveal(section: string, targets: string) {
	const root = document.querySelector(section);
	if (!root || still) return;
	const anims = [...document.querySelectorAll<HTMLElement>(targets)].map((el, i) =>
		el.animate(
			[
				{ opacity: 0.001, transform: 'translateY(20px)', filter: 'blur(4px)' },
				{ opacity: el.tagName === 'UL' ? 0.8 : 1, transform: 'translateY(0)', filter: 'blur(0)' },
			],
			{ duration: 300, delay: i * 150, easing: EASE, fill: 'both' },
		),
	);
	// A paused animation already shows its first keyframe, so the targets stay hidden until then.
	anims.forEach((a) => a.pause());
	const io = new IntersectionObserver((entries) => {
		if (!entries.some((e) => e.isIntersecting)) return;
		anims.forEach((a) => a.play());
		io.disconnect();
	});
	io.observe(root);
}

function fadeIn(targets: string, delay: (i: number) => number, duration: number) {
	document.querySelectorAll<HTMLElement>(targets).forEach((el, i) => {
		if (still) el.style.opacity = '1';
		else el.animate([{ opacity: 0 }, { opacity: 1 }], { duration, delay: delay(i), easing: OUT_QUAD, fill: 'both' });
	});
}

reveal('#header', '#header h1 b, #header p, #header div:has(a), #header .hero-media, #header ul');
reveal('#built-on', '#built-on h3, #built-on p, #built-on .logos a');
reveal('#community', '#community h2 span, #community p, #community .community__button, #community .values-media');

// The features section fades in on load, not on scroll.
fadeIn('.title-line', (i) => 200 + i * 200, 600);
fadeIn('.feature-description-text', () => 600, 600);
fadeIn('.feature-tab, .feature', (i) => 800 + i * 200, 500);

// Feature picker: the chosen card comes to the front, the rest tilt back into a stack.
const items = document.querySelectorAll<HTMLElement>('.feature, .feature-tab');
const description = document.querySelector<HTMLElement>('.feature-description');
const descriptions = description?.dataset.descriptions?.split('|||');
const media = document.querySelectorAll<HTMLElement>('.feature-video');

const mark = (el: Element, on: boolean) => (on ? el.setAttribute('data-active', 'true') : el.removeAttribute('data-active'));

function select(index: number) {
	items.forEach((el, i) => mark(el, i % 4 === index));
	if (description && descriptions) description.textContent = descriptions[index];
	media.forEach((el, i) => {
		const active = i === index;
		const depth = active ? 0 : -100 - Math.abs(i - index) * 50;
		mark(el, active);
		el.style.opacity = active ? '1' : '';
		el.style.transform = active
			? 'translate3d(-50%, 0, 0) scale(1)'
			: `translate3d(-50%, ${(i - index) * 20}px, ${depth}px) rotate3d(1, 0, 0, ${(i - index) * 3}deg) scale(0.95)`;
		el.style.zIndex = active ? '10' : String(1 - Math.abs(i - index));
		if (el instanceof HTMLVideoElement) {
			if (active) {
				el.currentTime = 0;
				void el.play();
			} else el.pause();
		}
	});
}
items.forEach((el) => el.addEventListener('click', () => select(Number(el.dataset.index))));
select(0);

// Nav dropdowns.
document.querySelectorAll<HTMLButtonElement>('.menu > button').forEach((button) => {
	button.addEventListener('click', () => {
		const open = button.getAttribute('aria-expanded') !== 'true';
		document.querySelectorAll<HTMLButtonElement>('.menu > button').forEach((b) => {
			b.setAttribute('aria-expanded', 'false');
			b.nextElementSibling?.setAttribute('hidden', '');
		});
		button.setAttribute('aria-expanded', String(open));
		button.nextElementSibling?.toggleAttribute('hidden', !open);
	});
});

// Theme switch; the initial theme is set inline in <head> to avoid a flash.
document.querySelector('.theme')?.addEventListener('click', () => {
	const dark = document.documentElement.dataset.theme !== 'dark';
	document.documentElement.dataset.theme = dark ? 'dark' : 'light';
	localStorage.setItem('landing-theme', dark ? 'dark' : 'light');
});
