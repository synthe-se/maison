// docs/ux/mise-en-page.md (Ariane) § 9 and docs/ux/tableau-de-bord.md « Ce que Maison
// applique » 8, measured on every page at every width: the title starts where the logo starts,
// the container is the same on every page, nothing scrolls sideways, the bottom bar is there
// only on a phone and never hides the focus, and no tile changes height when values refresh.
import { BASE, check, checkNoErrors, done, launch, onlyFailures, open, signIn } from './lib.ts';
import { house } from './house.ts';

const ROUTES = ['/', '/remote', '/tempo-predictions', '/device/feeder-1', '/hue-lamp/hue-01', '/meross/plug-1'];
const browser = await launch();
const p = await open(browser, { viewport: { width: 1440, height: 900 } });
const h = house();
await h.serve(p);
await signIn(p);
onlyFailures();

for (const w of [390, 1024, 1440, 2560]) {
	await p.setViewportSize({ width: w, height: 1000 });
	const widths: Record<string, number> = {};
	for (const r of ROUTES) {
		await p.goto(BASE + r);
		await p.locator('main h1').first().waitFor();
		await p.waitForTimeout(400);
		const g = await p.evaluate(() => {
			const main = document.querySelector('main')!;
			const cs = getComputedStyle(main);
			const box = main.getBoundingClientRect();
			const content = { left: box.left + parseFloat(cs.paddingLeft), width: box.width - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight) };
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

// values refresh without moving anything: every tile keeps its height across a poll (rule 8)
await p.setViewportSize({ width: 1440, height: 1000 });
await p.goto(BASE + '/');
await p.locator('article.tile').first().waitFor();
await p.waitForTimeout(500);
const heights = () => p.$$eval('article.tile', (ts) => ts.map((t) => Math.round(t.getBoundingClientRect().height)));
const before = await heights();
h.state.plugs[0].isOn = !h.state.plugs[0].isOn;
h.state.hue[0].state.brightness = 35;
await p.waitForTimeout(6000); // longer than the lamps' and plugs' poll (5 s)
const after = await heights();
check('no tile changes height when values refresh', JSON.stringify(before) === JSON.stringify(after), `${before} → ${after}`);

checkNoErrors();
await done(browser);
