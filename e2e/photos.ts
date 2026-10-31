// « Cadre photo »: an admin adds photos from a browser, resized there, sees them come back through
// the real backend, removes one once asked; what is no picture is refused with its words.
import { check, checkNoErrors, done, fr, launch, open, photoFile, shot, signIn, title, toast, visit } from './lib.ts';

const browser = await launch();
const page = await open(browser);
await signIn(page, '/account');
await page.getByRole('link', { name: fr.photos_title() }).click();
await title(page, fr.photos_title());
await page.getByText(fr.photos_none(), { exact: true }).waitFor();
check('with none, it says what the frame then does', await page.getByText(fr.photos_none_hint()).isVisible());

const picker = page.getByLabel(fr.photos_add());
await picker.setInputFiles([await photoFile(page, 'plage.png', '#e0a030'), await photoFile(page, 'neige.png', '#3060c0')]);
await toast(page, fr.photos_added({ count: 2 }));
const thumbs = page.getByRole('img', { name: /^Photo \d+$/ });
await thumbs.nth(1).waitFor();
await thumbs.nth(1).scrollIntoViewIfNeeded();
await page.waitForFunction(() => [...document.images].every((i) => i.complete));
const sizes = await thumbs.evaluateAll((all) =>
	all.map((i) => [(i as HTMLImageElement).naturalWidth, (i as HTMLImageElement).naturalHeight])
);
check(
	'both came back from the backend at the frame’s size',
	sizes.flat().every((n, i) => n === (i % 2 ? 1280 : 800)),
	JSON.stringify(sizes)
);
await shot(page, 'photos');

await picker.setInputFiles({ name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
await toast(page, new RegExp(fr.code_not_a_photo()));
check('what is no picture adds nothing', (await thumbs.count()) === 2);

await visit(page, '/photos');
check('they are still there on a new visit', (await thumbs.count()) === 2);
await page.getByRole('button', { name: fr.common_remove_named({ name: fr.photos_alt({ number: 1 }) }) }).click();
const asked = page.getByRole('alertdialog', { name: fr.photos_remove_title() });
await asked.getByRole('button', { name: fr.device_keep() }).click();
check('« Garder » keeps it', (await thumbs.count()) === 2);
await page.getByRole('button', { name: fr.common_remove_named({ name: fr.photos_alt({ number: 1 }) }) }).click();
await asked.getByRole('button', { name: fr.photos_remove_action() }).click();
await toast(page, fr.photos_removed());
await page.getByText(fr.photos_count({ count: 1 })).waitFor();
check('removed once asked', (await thumbs.count()) === 1);

checkNoErrors();
await done(browser);
