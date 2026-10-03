// docs/ux.md « Layout », § 4 and « Checks »,
// measured on every page at every width: the title starts where the logo starts,
// the container is the same on every page, nothing scrolls sideways, the bottom bar is there
// only on a phone and never hides the focus, and no tile changes height when values refresh.
import { BASE, check, checkNoErrors, done, invitation, launch, onlyFailures, open, settled, signIn, until } from './lib.ts';
import { house } from './house.ts';
import { showcase } from './showcase.ts';

const ROUTES = ['/', '/remote', '/tempo-predictions', '/device/feeder-1', '/hue-lamp/hue-01', '/meross/plug-1', '/tv', '/androidtv'];
const browser = await launch();

// the door (sign-in, an invitation), before any session: one column on a phone, two from
// 840 px, never sideways, the big action 52 px tall and in view
const door = await open(browser, { viewport: { width: 1440, height: 900 } });
const doorLink = invitation('door', 'Porte', false);
for (const w of [320, 390, 1440]) {
	await door.setViewportSize({ width: w, height: 800 });
	for (const [what, url] of [
		['sign-in', BASE + '/'],
		['invitation', doorLink]
	] as const) {
		await door.goto(url);
		await door.locator('main h1').first().waitFor();
		await settled(door);
		const g = await door.evaluate(() => {
			const side = document.querySelector('.door .side')!.getBoundingClientRect();
			const content = document.querySelector('.door .content')!.getBoundingClientRect();
			const big = document.querySelector<HTMLElement>('.door .big');
			return {
				sideways: document.documentElement.scrollWidth > innerWidth,
				columns: content.left >= side.right - 1,
				big: big ? Math.round(big.getBoundingClientRect().height) : 52
			};
		});
		check(`${w}px door ${what}: no sideways scroll`, !g.sideways);
		check(`${w}px door ${what}: ${w >= 840 ? 'two columns' : 'one column'}`, g.columns === w >= 840);
		check(`${w}px door ${what}: the big action is 52 px`, g.big === 52, `${g.big} px`);
	}
}
await door.context().close();

const p = await open(browser, { viewport: { width: 1440, height: 900 } });
const h = house();
await h.serve(p);
// the page's clock: real time, but a poll can be brought forward (below)
await p.clock.install();
await p.clock.resume();
await signIn(p);
onlyFailures();

for (const w of [390, 1024, 1440, 2560]) {
	await p.setViewportSize({ width: w, height: 1000 });
	const widths: Record<string, number> = {};
	for (const r of ROUTES) {
		await p.goto(BASE + r);
		await p.locator('main h1').first().waitFor();
		await settled(p);
		const g = await p.evaluate(() => {
			const main = document.querySelector('main')!;
			const cs = getComputedStyle(main);
			const box = main.getBoundingClientRect();
			const content = {
				left: box.left + parseFloat(cs.paddingLeft),
				width: box.width - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight)
			};
			const brand = document.querySelector('header .brand')!.getBoundingClientRect();
			const h1 = document.querySelector('main h1')!.getBoundingClientRect();
			const bottom = document.querySelector('nav.bottom');
			return {
				content,
				brandLeft: brand.left,
				h1Left: h1.left,
				sideways: document.documentElement.scrollWidth > innerWidth,
				bottomShown: !!bottom && getComputedStyle(bottom).display !== 'none',
				h1Count: document.querySelectorAll('main h1').length
			};
		});
		const at = `${w}px ${r}`;
		check(`${at}: the title starts where the logo starts`, Math.abs(g.h1Left - g.brandLeft) <= 1, `${g.h1Left} vs ${g.brandLeft}`);
		check(`${at}: content starts at the logo too`, Math.abs(g.content.left - g.brandLeft) <= 1, `${g.content.left} vs ${g.brandLeft}`);
		check(`${at}: no sideways scroll`, !g.sideways);
		check(`${at}: one h1`, g.h1Count === 1, String(g.h1Count));
		check(`${at}: bottom bar only on a phone`, g.bottomShown === w < 600);
		widths[r] = Math.round(g.content.width);
	}
	const all = new Set(Object.values(widths));
	check(`${w}px: the same container on every page`, all.size === 1, JSON.stringify(widths));

	// the focus is never hidden by a sticky bar (WCAG 2.4.11): tab to the last control of the home page
	await p.goto(BASE + '/');
	await p.locator('main h1').first().waitFor();
	await p.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
	const hidden = await p.evaluate(() => {
		const controls = [...document.querySelectorAll<HTMLElement>('main button, main a, main [role="slider"]')].filter((e) => e.offsetParent);
		const last = controls.at(-1)!;
		last.focus();
		last.scrollIntoView({ block: 'nearest' });
		const r = last.getBoundingClientRect();
		const bottom = document.querySelector('nav.bottom');
		const top = document.querySelector('header.top')!.getBoundingClientRect();
		const bar = bottom && getComputedStyle(bottom).display !== 'none' ? bottom.getBoundingClientRect().top : innerHeight;
		return r.top < top.bottom || r.bottom > bar;
	});
	check(`${w}px: a focused control is not hidden under a bar`, !hidden);
}

// reflow at 320 px (WCAG 1.4.10): the dashboard and a plug's page never scroll sideways
for (const r of ['/', '/meross/plug-1']) {
	await p.setViewportSize({ width: 320, height: 800 });
	await p.goto(BASE + r);
	await p.locator('main h1').first().waitFor();
	await settled(p);
	const wide = await p.evaluate(() => document.documentElement.scrollWidth - innerWidth);
	check(`320px ${r}: no sideways scroll`, wide <= 0, `${wide}px too wide`);
}

// values refresh without moving anything: every tile keeps its height across a poll (rule 8)
await p.setViewportSize({ width: 1440, height: 1000 });
await p.goto(BASE + '/');
await p.getByRole('article').first().waitFor();
await settled(p);
const heights = () => p.$$eval('article', (ts) => ts.map((t) => Math.round(t.getBoundingClientRect().height)));
const before = await heights();
h.state.plugs[0].isOn = !h.state.plugs[0].isOn;
h.state.hue[0].state.brightness = 35;
// the lamps' and plugs' next polls (5 s), brought forward on the page's clock, then answered
const asked = h.state.asked.length;
await p.clock.runFor(5_100);
await until(() => ['/api/meross', '/api/hue-lamps'].every((path) => h.state.asked.slice(asked).includes(path)));
await settled(p);
const after = await heights();
check('no tile changes height when values refresh', JSON.stringify(before) === JSON.stringify(after), `${before} → ${after}`);

// the phone dashboard stays compact (docs/ux.md « Layout »): the README's house at 390 px,
// rows of 56 px, « Maintenant » on two lines at most, and a bounded height
const PHONE_BOUND = 2300;
const phone = await open(browser, { viewport: { width: 390, height: 844 } });
await house().serve(phone);
await showcase(phone);
await signIn(phone);
await settled(phone);
const m = await phone.evaluate(() => {
	const chips = [...document.querySelectorAll<HTMLElement>('.now .now-chip')].filter((c) => c.offsetParent);
	const lines = new Set(chips.map((c) => Math.round(c.getBoundingClientRect().top))).size;
	// a lamp row with nothing under it (an off lamp): the row alone
	const off = [
		...document.querySelectorAll<HTMLElement>('#lamps-title ~ * article.tile, section[aria-labelledby="lamps-title"] article.tile')
	].find((t) => t.children.length === 1);
	return {
		height: Math.round(document.querySelector('main')!.getBoundingClientRect().height),
		page: document.documentElement.scrollHeight,
		lines,
		row: off ? Math.round(off.getBoundingClientRect().height) : 0,
		sideways: document.documentElement.scrollWidth > innerWidth,
		sticky: [...document.querySelectorAll('main *')].some((e) => ['sticky', 'fixed'].includes(getComputedStyle(e).position))
	};
});
console.log(
	`phone dashboard (390 px, README house): main ${m.height} px, page ${m.page} px, « Maintenant » on ${m.lines} line(s), a row ${m.row} px`
);
check(`390px: the dashboard is at most ${PHONE_BOUND} px tall`, m.height <= PHONE_BOUND, `${m.height} px`);
check('390px: « Maintenant » wraps to two lines at most', m.lines <= 2, `${m.lines} lines`);
check('390px: a device row is 56 px', m.row === 56, `${m.row} px`);
check('390px: no sideways scroll on the README house', !m.sideways);
check('390px: nothing sticky in the dashboard', !m.sticky);
await phone.setViewportSize({ width: 320, height: 800 });
await settled(phone);
const wide = await phone.evaluate(() => document.documentElement.scrollWidth - innerWidth);
check('320px: the README house never scrolls sideways', wide <= 0, `${wide}px too wide`);

checkNoErrors();
await done(browser);
