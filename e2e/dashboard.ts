// The dashboard against a simulated house (house.ts): what a tile says, what a gesture sends,
// what the page does when a device does not answer (docs/ux.md § 1–4). The words come from the
// app's French messages (`fr`); the timings that matter (a command's 1 s and 3 s, a shutter's
// 400 ms after the last key) run on the page's own clock (`page.clock`), never on a sleep.
import { BASE, check, checkNoErrors, done, fr, launch, open, shot, signIn, toast, until } from './lib.ts';
import { house } from './house.ts';

const browser = await launch();
const p = await open(browser);
const h = house();
await h.serve(p);
// the page's clock: real time by default (resumed), paused where a timing is checked
await p.clock.install();
await p.clock.resume();
await signIn(p);
/** Stops the page's clock (a moment ahead of its own now): from here, time moves by `runFor`. */
const pause = async () => p.clock.pauseAt((await p.evaluate(() => Date.now())) + 100);

const tile = (name: string) => p.getByRole('article', { name, exact: true });
const gesture = (name: string) => tile(name).getByRole('button', { name, exact: true });
const lastSent = (expected: string) => until(() => h.state.sent.at(-1) === expected);

// ── a lamp: the icon is the pressed on/off button, named by the lamp ──
await tile('Salon').waitFor();
check('the lamp says its state in words', (await tile('Salon').innerText()).includes(fr.lamps_on_percent({ percent: 80 })));
check('its icon is a pressed button named after it', (await gesture('Salon').getAttribute('aria-pressed')) === 'true');
await gesture('Salon').click();
check('pressed: shows the target at once', (await gesture('Salon').getAttribute('aria-pressed')) === 'false');
check('one command sent, off', await lastSent('POST /api/hue-lamps/hue-01/power {"enabled":false}'), h.state.sent.at(-1));

// an unreachable lamp: its gesture kept in place but unavailable, its state and since when in words
check(
	'unreachable lamp: says since when',
	(await tile('Chambre').innerText()).includes(fr.state_unreachable_for({ duration: fr.duration_minutes({ m: 12 }) }))
);
check('unreachable lamp: the on/off button stays, unavailable', (await gesture('Chambre').getAttribute('aria-disabled')) === 'true');

// a lamp that stops answering: « Pas de réponse » after the lamp limit (3 s), back to the read state
h.state.silent.add('hue');
const before = await gesture('Salon').getAttribute('aria-pressed');
await pause();
await gesture('Salon').click();
await p.clock.runFor(1_000);
const working = await tile('Salon').innerText();
check(
	'after 1 s the tile says it is working',
	working.includes(fr.command_turning_on()) || working.includes(fr.command_turning_off()),
	working
);
await p.clock.runFor(2_000);
await tile('Salon').getByText(fr.command_no_answer_short()).waitFor();
check('after the limit: « Pas de réponse »', true);
check('and back to the state read from the lamp', (await gesture('Salon').getAttribute('aria-pressed')) === before);
check('a retry is offered', await tile('Salon').getByRole('button', { name: fr.common_retry() }).isVisible());
await p.clock.resume();
h.state.silent.delete('hue');
await shot(tile('Salon'), 'lamp-no-answer');

// ── plugs: power as the one fact, the offline one says so ──
await tile('Lave-linge').waitFor();
check('a plug offline says so', (await tile('Radiateur').innerText()).includes(fr.state_unreachable()));
await gesture('Lave-linge').click();
check('plug toggled off', await until(() => h.state.plugs[0].isOn === false), h.state.sent.at(-1));

// ── the shutter: open/stop/close and a position read in words ──
const shutter = tile('Volet salon');
await shutter.getByRole('button', { name: fr.shutters_close_action(), exact: true }).click();
check('close sent', await until(() => h.state.sent.includes('POST /api/matter/covers/0000000000000002/close')));
await shutter.getByText(fr.shutters_closed(), { exact: true }).first().waitFor();
check('the tile says closed', true);
const slider = shutter.getByRole('slider');
await slider.focus();
// the motor gets its target 0.4 s after the last key, not one per key (§ 3)
await pause();
await p.keyboard.press('PageUp');
await p.clock.runFor(150);
check('no target sent while keys may still come', !h.state.sent.some((x) => x.includes('/position')));
await p.clock.runFor(300);
check('Page Up moves 25 %', await lastSent('POST /api/matter/covers/0000000000000002/position {"openPercent":25}'), h.state.sent.at(-1));
await p.clock.resume();
check('the slider speaks words', (await slider.getAttribute('aria-valuetext')) === fr.shutters_open_percent({ percent: 25 }));

// ── group actions: every lamp off at once, every shutter closed, the outcome said ──
h.state.hue[0].state.isOn = true;
await p.reload();
await tile('Salon').waitFor();
await p.getByRole('button', { name: fr.lamps_all_off() }).click();
await toast(p, fr.lamps_all_off_done({ count: 1 }));
check(
	'« Tout éteindre »: the lit lamp turned off',
	h.state.sent.at(-1) === 'POST /api/hue-lamps/hue-01/power {"enabled":false}',
	h.state.sent.at(-1)
);
check(
	'« Tout éteindre » gone, the focus on the group’s title',
	await until(() => p.evaluate(() => document.activeElement?.id === 'lamps-title'))
);
await p.getByRole('button', { name: fr.shutters_all_open() }).click();
await toast(p, fr.shutters_all_opening({ count: 1 }));
check('« Tout ouvrir »: the shutter opens', h.state.sent.at(-1) === 'POST /api/matter/covers/0000000000000002/open', h.state.sent.at(-1));

// ── skipping tonight's closing once, then the undo ──
await shutter.getByRole('button', { name: fr.shutters_skip_close_tonight() }).click();
await shutter.getByRole('button', { name: fr.shutters_skip_undo() }).waitFor();
check(
	'skip sent',
	h.state.sent.at(-1) === 'POST /api/matter/covers/0000000000000002/skip {"event":"close","skip":true}',
	h.state.sent.at(-1)
);
await shutter.getByRole('button', { name: fr.shutters_skip_undo() }).click();
await shutter.getByRole('button', { name: fr.shutters_skip_close_tonight() }).waitFor();
check(
	'undo sent',
	h.state.sent.at(-1) === 'POST /api/matter/covers/0000000000000002/skip {"event":"close","skip":false}',
	h.state.sent.at(-1)
);

// ── the cats: line 2 says what matters, the offline litter box cannot clean ──
check('the fountain says its water is low', (await tile('Fontaine').innerText()).includes(fr.cats_water_low()));
const now = p.getByRole('region', { name: fr.now_title() });
const lowWater = now.getByRole('button', { name: fr.now_alert({ name: 'Fontaine', what: fr.cats_water_low().toLocaleLowerCase('fr') }) });
check('« Maintenant » says it too', await lowWater.isVisible());
await lowWater.click();
check('a chip leads to its group', await until(() => p.evaluate(() => document.activeElement?.id === 'cats-title')));

// ── scenes: an admin makes « Je pars » from its template, then runs it ──
await p.getByRole('button', { name: fr.scenes_add() }).click();
const sheet = p.getByRole('dialog', { name: fr.scenes_new() });
await sheet.waitFor();
await sheet.getByRole('group', { name: fr.scenes_from_template() }).getByRole('button', { name: fr.scenes_template_leave() }).click();
const template = await sheet.innerText();
check('the template is filled from the house’s devices', template.includes('Lave-linge'));
check(
	'…the AC off and the shutters closed too',
	template.includes(fr.remote_action_types_climate_off()) &&
		template.includes('Volet salon') &&
		template.includes(fr.shutters_close_action()),
	template
);
await sheet.getByRole('button', { name: fr.common_save() }).click();
await toast(p, fr.scenes_saved({ name: fr.scenes_template_leave() }));
const run = p.getByRole('button', { name: fr.scenes_template_leave(), exact: true });
await run.waitFor();
await run.click();
// the throwaway backend has no plug, shutter nor TV: each action fails, and the warning names them
await toast(p, new RegExp(fr.scenes_template_leave()));
check(
	'the run says how it went',
	(await p.locator('.toast').filter({ hasText: fr.scenes_template_leave() }).last().innerText()).length > 0
);
// an app shortcut asks first, never runs silently
await p.goto(BASE + '/?scene=je-pars');
const ask = p.getByRole('alertdialog', { name: fr.scenes_run_title({ name: fr.scenes_template_leave() }) });
await ask.waitFor();
check('a shortcut asks before running a scene', await ask.isVisible());
await ask.getByRole('button', { name: fr.device_keep() }).click();
check('…and « Garder » runs nothing, the address forgets it', await until(() => !p.url().includes('scene=')));

// ── a settings switch keeps the focus while its order travels (never disabled under the finger) ──
await p.goto(BASE + '/device/feeder-1');
await p.getByRole('heading', { name: 'Pixi Feeder', level: 1 }).waitFor();
await p.getByRole('tab', { name: fr.feeder_schedule() }).click();
const meal = p.getByRole('tabpanel').getByRole('switch').first();
await meal.focus();
await p.keyboard.press('Space');
check('the switch says it is busy', await until(() => meal.evaluate((el) => el.getAttribute('aria-busy') === 'true')));
check('…and keeps the focus', await meal.evaluate((el) => el === document.activeElement));
check('…until saved', await until(() => meal.evaluate((el) => !el.hasAttribute('aria-busy'))));
check('…still focused once saved', await meal.evaluate((el) => el === document.activeElement));
await p.getByRole('link', { name: fr.back_home() }).click();
await p.getByRole('heading', { name: fr.nav_home(), level: 1 }).waitFor();

// ── the cats: the device links to its page ──
await p.getByRole('link', { name: 'Pixi Feeder' }).click();
await p.getByRole('heading', { name: 'Pixi Feeder', level: 1 }).waitFor();
check('the feeder has its page', p.url().endsWith('/device/feeder-1'));
await p.getByRole('link', { name: fr.back_home() }).click();
await p.getByRole('heading', { name: fr.nav_home(), level: 1 }).waitFor();

await shot(p, 'dashboard', true);
checkNoErrors();
await done(browser);
