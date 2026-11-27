import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { chromium } from 'playwright';
import { chromiumOptions, database, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const caseMode = process.argv.includes('--case');
if (caseMode) {
  const packet = JSON.parse(readFileSync(0, 'utf8'));
  assert.equal(packet.case, 'photo-match');
  assert.equal(packet.input.fixture, 'household');
}
const fixtures = JSON.parse(readFileSync(new URL('../../fixtures/assets.json', import.meta.url), 'utf8'));
const prefix = `photos-${randomUUID().slice(0, 8)}-`;
const { households, actors } = provision(prefix);
const browser = await chromium.launch(chromiumOptions);
const button = (page, name) => page.getByRole('button', { name, exact: true });
const visible = (locator) => locator.waitFor({ state: 'visible', timeout: 10000 });
const sql = (query) => execFileSync('psql', [
  '--dbname', database, '-X', '-v', 'ON_ERROR_STOP=1', '-Atqc', query,
], { encoding: 'utf8' }).trim();

async function signIn(alias) {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(origin);
  await virtualAuthenticator(page);
  await button(page, 'I have a passkey setup code').click();
  await page.getByLabel('Passkey setup code').fill(run('account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').click();
  await visible(page.getByText('Passkey added. Sign in with your account name.'));
  await page.getByLabel('Account name').fill(prefix + alias);
  await page.getByLabel('Account name').press('Enter');
  await visible(page.getByRole('heading', { name: 'Home', exact: true }));
  return page;
}

async function request(page, path, method = 'GET', body, csrf = true) {
  return page.evaluate(async ({ path, method, body, csrf }) => {
    const headers = { 'Content-Type': 'application/json' };
    if (csrf) {
      const session = await fetch('/api/v1/session').then((response) => response.json());
      headers['X-CSRF-Token'] = session.csrf_token;
    }
    const response = await fetch(`/api/v1${path}`, { method, headers, body: JSON.stringify(body) });
    return { status: response.status, body: response.status === 204 ? null : await response.json() };
  }, { path, method, body, csrf });
}

async function homeAction(page, action) {
  const home = button(page, 'Home');
  await home.click();
  await button(page, action).click();
}

async function writeMoment(page, note) {
  await homeAction(page, 'Write a sentence');
  await page.getByLabel('What happened?').fill(note);
  await button(page, 'Save moment').click();
  await visible(page.getByText('Moment saved.', { exact: true }));
  const list = await request(page, '/moments');
  const moment = list.body.find((entry) => entry.note === note);
  assert(moment);
  assert.deepEqual(moment.asset_ids, []);
  return moment;
}

const inbox = '/family/photo-inbox';
const photo = fixtures.find((fixture) => fixture.id === 'photo');

async function queue(parent, fixture, interrupt = false) {
  await homeAction(parent, 'Family photos to match');
  const picker = fixture === photo ? 'Choose photos' : 'Choose a file';
  const file = readFileSync(new URL(`../../fixtures/${fixture.path}`, import.meta.url));
  const label = fixture.path.split('/').at(-1);
  let offset = 0;
  if (interrupt) {
    await parent.route('**/api/v1/uploads/*/parts/*', async (route) => {
      offset = Number(route.request().url().split('/').at(-1));
      if (offset !== 0) return route.continue();
      const response = await route.fetch({ postData: file.subarray(0, Math.floor(file.length / 2)) });
      assert.equal(response.status(), 200);
      await route.abort('connectionreset');
    });
  }
  const [chooser] = await Promise.all([
    parent.waitForEvent('filechooser'),
    button(parent, picker).press('Enter'),
  ]);
  await chooser.setFiles({ name: label, mimeType: fixture.mime, buffer: file });
  await parent.getByLabel('Photo activity date (optional)').fill('2026-02-05');
  await button(parent, 'Save for matching').press('Enter');
  if (interrupt) {
    await visible(parent.getByRole('alert').filter({ hasText: 'Use Save to retry' }));
    await button(parent, 'Save for matching').press('Enter');
  }
  await visible(parent.getByText('File saved to Family photos to match.'));
  await parent.unroute('**/api/v1/uploads/*/parts/*');
  if (interrupt) assert.equal(offset, Math.floor(file.length / 2));
  await visible(parent.getByText('Uploaded by Fixture parent-one · 2026-02-05', { exact: true }));
  const response = await request(parent, `${inbox}/${actors.learner}`);
  const asset = response.body.find((item) => item.label === label);
  assert.equal(asset.uploader_id, actors['parent-one']);
  assert.equal(asset.activity_date, '2026-02-05');
  return { asset, offset };
}

function count(query) {
  return Number(sql(query));
}

function original(asset, fixture) {
  const result = sql(`SELECT uploader_id, byte_count, encode(sha256, 'hex') FROM assets WHERE id = '${asset.id}'`);
  assert.equal(result, [actors['parent-one'], fixture.byte_length, fixture.sha256].join('|'));
}

async function openMoment(page, note) {
  await homeAction(page, 'Family journal');
  const back = button(page, 'Back to family journal');
  if (await back.count()) await back.click();
  await page.getByRole('button').filter({ hasText: note }).click();
  await visible(page.getByRole('heading', { name: 'Moment', exact: true }));
}

try {
  const learner = await signIn('learner');
  const parent = await signIn('parent-one');
  const photoFirst = caseMode;
  let moment = photoFirst ? null : await writeMoment(learner, 'I lined up the fraction strips.');
  const before = count(`SELECT count(*) FROM assets WHERE household_id = '${households.A}'`);
  const { asset, offset } = await queue(parent, photo, true);
  const newOriginals = count(`SELECT count(*) FROM assets WHERE household_id = '${households.A}'`) - before;
  assert.equal(newOriginals, 1);
  original(asset, photo);
  if (!moment) moment = await writeMoment(learner, 'I checked the two halves.');
  else await openMoment(learner, moment.note);
  assert.deepEqual((await request(learner, `/moments/${moment.id}`)).body.asset_ids, []);
  const path = `${inbox}/${asset.id}/match/${moment.id}`;

  await button(learner, `Match ${asset.label}`).press('Enter');
  await visible(learner.getByText('File matched.', { exact: true }));
  const replay = await Promise.all([request(learner, path, 'POST'), request(parent, path, 'POST')]);
  assert.deepEqual(replay.map(({ status }) => status), [204, 204]);
  const matchesBefore = count(`SELECT count(*) FROM moment_assets WHERE asset_id = '${asset.id}'`);
  assert.equal(matchesBefore, 1);
  await visible(learner.getByText('Matched by Fixture learner', { exact: true }));
  assert.equal((await request(learner, `/moments/${moment.id}/attachments`)).body[0].matched_by, actors.learner);

  await button(learner, `Unmatch ${asset.label}`).press('Enter');
  await visible(learner.getByText('File returned to Family photos to match.', { exact: true }));
  const matchesAfter = count(`SELECT count(*) FROM moment_assets WHERE asset_id = '${asset.id}'`);
  assert.equal(matchesAfter, 0);
  assert.equal((await request(learner, path, 'DELETE')).status, 204);
  original(asset, photo);
  assert.equal((await request(learner, `${inbox}/${actors.learner}`)).body[0].id, asset.id);

  if (caseMode) {
    console.log(JSON.stringify({
      new_originals: newOriginals, matches_before_unmatch: matchesBefore, matches_after_unmatch: matchesAfter,
      uploader: 'parent-one', original_hash_preserved: true,
    }));
  } else {
    const diary = await writeMoment(learner, 'My private diary fixture.');
    assert.equal((await request(parent, `${inbox}/${asset.id}/match/${diary.id}`, 'POST')).status, 204);
    assert.equal((await request(learner, path, 'POST')).status, 409);
    assert.equal((await request(parent, `${inbox}/${asset.id}/match/${diary.id}`, 'DELETE')).status, 204);
    // B4 adds diary creation. Temporarily admit a synthetic diary row to test
    // the existing server filter against a real ID in this disposable database.
    sql(`ALTER TABLE moments DROP CONSTRAINT moments_visibility_check;
         UPDATE moments SET visibility = 'diary' WHERE id = '${diary.id}'`);
    try {
      const diaryPath = `${inbox}/${asset.id}/match/${diary.id}`;
      assert.equal((await request(parent, diaryPath, 'POST')).status, 404);
      assert.equal((await request(parent, `/moments/${diary.id}/attachments`)).status, 404);
      assert.equal(count(`SELECT count(*) FROM moment_assets WHERE moment_id = '${diary.id}'`), 0);
    } finally {
      sql(`UPDATE moments SET visibility = 'family' WHERE id = '${diary.id}';
           ALTER TABLE moments ADD CONSTRAINT moments_visibility_check CHECK (visibility = 'family')`);
    }

    const outsider = await signIn('other-learner');
    assert.equal((await request(outsider, `${inbox}/${actors.learner}`)).status, 404);
    assert.equal((await request(outsider, path, 'POST')).status, 404);
    assert.equal((await request(learner, path, 'POST', undefined, false)).status, 403);
    await openMoment(learner, moment.note);
    await button(learner, `Match ${asset.label}`).press('Enter');
    await visible(learner.getByText('File matched.', { exact: true }));
    original(asset, photo);
    assert.equal(count(`SELECT count(*) FROM assets WHERE household_id = '${households.A}'`), 1);
    assert.equal(sql(`SELECT string_agg(matched::text, ',' ORDER BY recorded_at) FROM family_photo_match_events
                       WHERE asset_id = '${asset.id}' AND moment_id = '${moment.id}'`), 'true,false,true');

    const letter = fixtures.find((fixture) => fixture.id === 'letter');
    const { asset: pdf } = await queue(parent, letter);
    const later = await writeMoment(learner, 'I kept a letter about the project.');
    await button(learner, `Match ${pdf.label}`).press('Enter');
    await visible(learner.getByText('File matched.', { exact: true }));
    original(pdf, letter);
    assert.deepEqual((await request(learner, `/moments/${later.id}`)).body.asset_ids, [pdf.id]);

    await openMoment(parent, later.note);
    const demo = fixtures.find((fixture) => fixture.id === 'demo');
    const [chooser] = await Promise.all([
      parent.waitForEvent('filechooser'), button(parent, 'Choose a file').press('Enter'),
    ]);
    await chooser.setFiles({
      name: 'short-demo.mp4', mimeType: demo.mime,
      buffer: readFileSync(new URL(`../../fixtures/${demo.path}`, import.meta.url)),
    });
    await button(parent, 'Save and attach').press('Enter');
    await visible(parent.getByText('File attached.', { exact: true }));
    const linked = (await request(parent, `/moments/${later.id}/attachments`)).body;
    assert.deepEqual(linked.map((item) => item.label), [pdf.label, 'short-demo.mp4']);
    assert.equal(linked[1].matched_by, actors['parent-one']);
    original(linked[1], demo);
    console.log('B3.2 fixture', {
      household: households.A, guardian: actors['parent-one'], learner: actors.learner, wordsFirst: moment.id,
      photo: asset.id, resumeOffset: offset, privateDiaryFixture: diary.id,
      photoFirst: later.id, letter: pdf.id, directlyAttachedDemo: linked[1].id, browser: browser.version(),
    });
  }
} finally {
  await browser.close();
}
