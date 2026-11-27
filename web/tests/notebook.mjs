import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium, devices } from 'playwright';
import { chromiumOptions, database, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const { actors } = provision('notebook-');
const scratch = await mkdtemp(join(tmpdir(), 'fieldbook-zoom-'));
const extension = join(scratch, 'zoom');
await mkdir(extension);
await writeFile(join(extension, 'manifest.json'), JSON.stringify({
  manifest_version: 3, name: 'Test tab zoom', version: '1.0',
  permissions: ['tabs'], background: { service_worker: 'zoom.js' },
}));
await writeFile(join(extension, 'zoom.js'), 'chrome.runtime.onInstalled.addListener(() => {});');

const options = {
  ...chromiumOptions,
  ignoreDefaultArgs: ['--disable-back-forward-cache'],
};
const desktop = await chromium.launchPersistentContext(join(scratch, 'profile'), {
  ...options,
  channel: 'chromium',
  viewport: { width: 720, height: 900 },
  args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
});
const mobileBrowser = await chromium.launch(chromiumOptions);
const page = desktop.pages()[0];
const pageErrors = [];
page.on('pageerror', (failure) => pageErrors.push(failure));
await desktop.addInitScript(() => {
  window.fieldbookRestoredPage = false;
  addEventListener('pageshow', (event) => { window.fieldbookRestoredPage = event.persisted; });
});

const name = (page) => page.getByLabel('Account name', { exact: true });
const heading = (page, text) => page.getByRole('heading', { level: 1, name: text, exact: true });
const button = (page, label) => page.getByRole('button', { name: label, exact: true });
const visible = (locator) => locator.waitFor({ state: 'visible', timeout: 10000 });
const focus = (target) => target.evaluate((element) => element === document.activeElement);
const waitForFocus = (target) => target.evaluate(async (element) => {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    if (element === document.activeElement) return true;
    await new Promise(requestAnimationFrame);
  }
  return false;
});

async function addPasskey(page, alias) {
  await button(page, 'I have a passkey setup code').click();
  const token = page.getByLabel('Passkey setup code');
  await token.fill(run('account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').click();
  await visible(page.getByText('Passkey added. Sign in with your account name.'));
  assert(await waitForFocus(name(page)));
}

async function signIn(page, alias, keyboard = false) {
  await visible(name(page));
  if (keyboard) {
    assert(await waitForFocus(name(page)));
    await page.keyboard.type(`notebook-${alias}`);
    await page.keyboard.press('Tab');
    assert(await focus(button(page, 'Sign in with a passkey')));
    await page.keyboard.press('Enter');
  } else {
    await name(page).fill(`notebook-${alias}`);
    await button(page, 'Sign in with a passkey').click();
  }
  await visible(heading(page, 'Home'));
  assert(await waitForFocus(heading(page, 'Home')));
  await visible(page.getByText(`Fixture ${alias}`, { exact: true }));
}

async function writer(page) {
  await page.keyboard.press('Tab');
  assert(await focus(button(page, 'Write a sentence')));
  await page.keyboard.press('Enter');
  const field = page.getByRole('textbox', { name: 'What happened?' });
  await visible(field);
  assert(await waitForFocus(field));
  return field;
}

async function assertFits(page) {
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  for (const control of await page.getByRole('button').all()) {
    const box = await control.boundingBox();
    assert(box.width >= 44 && box.height >= 44);
  }
}

try {
  const response = await page.goto(origin);
  assert.equal(response.headers()['cache-control'], 'no-store, private');
  await visible(heading(page, 'Sign in'));
  await assertFits(page);
  await virtualAuthenticator(page);

  await addPasskey(page, 'learner');
  await page.setViewportSize({ width: 360, height: 900 });
  assert.equal(await page.evaluate(() => innerWidth), 360);
  await signIn(page, 'learner', true);
  await assertFits(page);
  const field = await writer(page);
  await page.keyboard.type('Fictional private draft for the learner.');

  // The second tab ends the same cookie-backed session after the learner page enters history.
  await page.goto(origin + '/healthz');
  const secondTab = await desktop.newPage();
  await secondTab.goto(origin);
  await visible(heading(secondTab, 'Home'));
  await secondTab.keyboard.press('Shift+Tab');
  await secondTab.keyboard.press('Shift+Tab');
  await secondTab.keyboard.press('Shift+Tab');
  await secondTab.keyboard.press('Shift+Tab');
  assert(await focus(button(secondTab, 'Switch account')));
  await secondTab.keyboard.press('Enter');
  await visible(secondTab.getByText('Sign in with a different account.'));
  assert(await waitForFocus(name(secondTab)));
  await virtualAuthenticator(secondTab);
  await addPasskey(secondTab, 'parent-one');
  await signIn(secondTab, 'parent-one', true);
  await secondTab.close();
  await page.goBack();
  await visible(heading(page, 'Home'));
  await visible(page.getByText('Fixture parent-one', { exact: true }));
  assert.equal(await page.getByText('Fixture learner', { exact: true }).count(), 0);
  assert.equal(await field.count(), 0);
  const restoredAfterSwitch = await page.evaluate(() => window.fieldbookRestoredPage);
  assert.deepEqual(await page.evaluate(() => [localStorage.length, sessionStorage.length]), [0, 0]);

  // Chrome tab zoom changes layout pixels and devicePixelRatio, unlike pinch scaling.
  await page.setViewportSize({ width: 720, height: 900 });
  const zoom = desktop.serviceWorkers()[0] ?? await desktop.waitForEvent('serviceworker');
  await zoom.evaluate(async (site) => {
    const [tab] = (await chrome.tabs.query({})).filter(({ url }) => url === `${site}/`);
    await chrome.tabs.setZoom(tab.id, 2);
  }, origin);
  await page.waitForFunction(() => innerWidth === 360 && devicePixelRatio === 2);
  assert.equal(await page.getByText('Fixture learner', { exact: true }).count(), 0);
  await visible(page.getByRole('heading', { name: 'Working together' }));
  await assertFits(page);
  await button(page, 'Projects').click();
  await visible(heading(page, 'Projects'));
  await visible(page.getByText('No projects yet.'));
  await button(page, 'Home').click();
  const parentField = await writer(page);
  assert.equal(await parentField.inputValue(), '');
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Shift+Tab');
  assert(await focus(button(page, 'Sign out')));
  await page.keyboard.press('Enter');
  await visible(page.getByText('You signed out.'));
  console.log(`Shared Chromium keyboard path at 360 CSS px and 200% tab zoom. Back restored BFCache: ${restoredAfterSwitch}`);

  const mobile = await mobileBrowser.newContext(devices['Pixel 7']);
  const parentPage = await mobile.newPage();
  parentPage.on('pageerror', (failure) => pageErrors.push(failure));
  await parentPage.goto(origin);
  await virtualAuthenticator(parentPage, 'internal');
  await addPasskey(parentPage, 'parent-two');
  await signIn(parentPage, 'parent-two');
  await button(parentPage, 'Verify it’s you').tap();
  await visible(parentPage.getByText('Identity verified.'));
  await button(parentPage, 'Write a sentence').tap();
  await parentPage.getByRole('textbox', { name: 'What happened?' }).fill('Another fictional draft.');
  const psql = process.env.FIELDBOOK_TEST_PSQL ?? 'psql';
  assert.match(actors['parent-two'], /^[0-9a-f-]{36}$/);
  execFileSync(psql, ['--dbname', database, '-X', '-v', 'ON_ERROR_STOP=1', '-qc',
    `UPDATE sessions SET idle_until = now() - interval '1 second' WHERE actor_id = '${actors['parent-two']}'`]);
  await parentPage.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await visible(parentPage.getByText('Your session ended. Sign in to continue.'));
  assert.equal(await parentPage.getByRole('textbox', { name: 'What happened?' }).count(), 0);
  assert.equal(await parentPage.getByText('Fixture parent-two', { exact: true }).count(), 0);
  assert(await waitForFocus(name(parentPage)));
  assert.deepEqual(pageErrors, []);
  console.log(`Pixel 7 viewport/touch emulation in ${mobileBrowser.version()} with a virtual platform passkey. Step-up and expired-session draft clearing passed.`);
  console.log('Notebook fixture actors', actors);
} finally {
  await mobileBrowser.close();
  await desktop.close();
  await rm(scratch, { recursive: true, force: true });
}
