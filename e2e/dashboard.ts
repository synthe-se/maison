// The dashboard against a simulated house (house.ts): what a tile says, what a gesture sends,
// what the page does when a device does not answer (docs/ux/tableau-de-bord.md § 1–4).
import { check, checkNoErrors, done, launch, open, shot, signIn } from './lib.ts';
import { house } from './house.ts';

const browser = await launch();
const p = await open(browser);
const h = house();
await h.serve(p);
await signIn(p);

const tile = (name: string) => p.getByRole('article', { name, exact: true });
const gesture = (name: string) => tile(name).getByRole('button', { name, exact: true });

// ── a lamp: the icon is the pressed on/off button, named by the lamp ──
await tile('Salon').waitFor();
check('the lamp says its state in words', /Allumée/.test(await tile('Salon').innerText()));
check('its icon is a pressed button named after it', (await gesture('Salon').getAttribute('aria-pressed')) === 'true');
await gesture('Salon').click();
check('pressed: shows the target at once', (await gesture('Salon').getAttribute('aria-pressed')) === 'false');
await p.waitForTimeout(300);
check('one command sent, off', h.state.sent.at(-1) === 'POST /api/hue-lamps/hue-01/power {"enabled":false}', h.state.sent.at(-1));

// an unreachable lamp: no gesture offered, its state in words
check('unreachable lamp: says so', /Injoignable/.test(await tile('Chambre').innerText()));
check('unreachable lamp: no on/off button', (await gesture('Chambre').count()) === 0);

// a lamp that stops answering: « Pas de réponse » after the lamp limit (3 s), back to the read state
h.state.silent.add('hue');
const before = await gesture('Salon').getAttribute('aria-pressed');
await gesture('Salon').click();
await p.waitForTimeout(1300);
check('after 1 s the tile says it is working', /Allumage|Extinction/.test(await tile('Salon').innerText()));
await p.waitForTimeout(2200);
check('after the limit: « Pas de réponse »', /Pas de réponse/.test(await tile('Salon').innerText()));
check('and back to the state read from the lamp', (await gesture('Salon').getAttribute('aria-pressed')) === before);
check('a retry is offered', await tile('Salon').getByRole('button', { name: 'Réessayer' }).isVisible());
h.state.silent.delete('hue');
await shot(tile('Salon'), 'lamp-no-answer');

// ── plugs: power as the one fact, the offline one says so ──
await tile('Lave-linge').waitFor();
check('a plug offline says so', /Injoignable|Hors ligne/.test(await tile('Radiateur').innerText()));
await gesture('Lave-linge').click();
await p.waitForTimeout(300);
check('plug toggled off', h.state.plugs[0].isOn === false, h.state.sent.at(-1));

// ── the shutter: open/stop/close and a position read in words ──
const shutter = tile('Volet salon');
await shutter.getByRole('button', { name: 'Fermer' }).click();
await p.waitForTimeout(300);
check('close sent', h.state.sent.includes('POST /api/matter/covers/0000000000000002/close'));
check('the tile says closed', /Fermé/.test(await shutter.innerText()));
const slider = shutter.getByRole('slider');
await slider.focus();
await p.keyboard.press('PageUp');
await p.waitForTimeout(400);
check('Page Up moves 25 %', h.state.sent.at(-1) === 'POST /api/matter/covers/0000000000000002/position {"openPercent":25}', h.state.sent.at(-1));
check('the slider speaks words', (await slider.getAttribute('aria-valuetext')) === 'Ouvert à 25 %');

// ── the cats: the device links to its page ──
await p.getByRole('link', { name: 'Pixi Feeder' }).click();
await p.getByRole('heading', { name: 'Pixi Feeder', level: 1 }).waitFor();
check('the feeder has its page', p.url().endsWith('/device/feeder-1'));
await p.getByRole('link', { name: 'Retour à l’accueil' }).click();
await p.getByRole('heading', { name: 'Accueil', level: 1 }).waitFor();

await shot(p, 'dashboard', true);
checkNoErrors();
await done(browser);
