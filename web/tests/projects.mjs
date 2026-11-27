import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { chromium } from 'playwright';
import { chromiumOptions, database, origin, provision, runIn, virtualAuthenticator } from './fixture.mjs';

const fixture = JSON.parse(readFileSync(new URL('../../fixtures/household.json', import.meta.url)));
const asset = JSON.parse(readFileSync(new URL('../../fixtures/assets.json', import.meta.url))).find(({ id }) => id === 'photo');
const prefix = `projects-${randomUUID().slice(0, 8)}-`;
const { households, actors } = provision(prefix);
const browser = await chromium.launch(chromiumOptions);
const button = (page, name) => page.getByRole('button', { name, exact: true });
const text = (page, name) => page.getByText(name, { exact: true });
const visible = (locator) => locator.waitFor({ state: 'visible', timeout: 10000 });
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const sql = (url, query) => execFileSync('psql', ['--dbname', url, '-X', '-v', 'ON_ERROR_STOP=1', '-Atqc', query],
  { encoding: 'utf8' }).trim();

async function signIn(alias, base = origin, fixtureDatabase = database) {
  const context = await browser.newContext({ baseURL: base, acceptDownloads: true, viewport: { width: 390, height: 844 } });
  const page = await context.newPage();
  await page.goto(base);
  await virtualAuthenticator(page);
  await button(page, 'I have a passkey setup code').press('Enter');
  await page.getByLabel('Passkey setup code').fill(runIn(fixtureDatabase, 'account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').press('Enter');
  await visible(text(page, 'Passkey added. Sign in with your account name.'));
  await page.getByLabel('Account name').fill(prefix + alias);
  await page.getByLabel('Account name').press('Enter');
  await visible(page.getByRole('heading', { name: 'Home', exact: true }));
  return page;
}

async function api(page, route, method = 'GET', data, base = origin) {
  const session = await page.request.get(`${base}/api/v1/session`).then((response) => response.json());
  return page.request.fetch(`${base}/api/v1${route}`, {
    method, headers: { Origin: base, 'X-CSRF-Token': session.csrf_token }, data,
  });
}

async function catalog(page) {
  return api(page, '/projects').then((r) => r.json());
}

async function createMoment(page, note, date) {
  const response = await api(page, '/moments', 'POST', { submission_key: randomUUID(), note, activity_date: date });
  assert.equal(response.status(), 201);
  return response.json();
}

function timelineRow(page, note) {
  return page.getByRole('list', { name: 'Activity order' }).getByRole('listitem')
    .filter({ has: page.getByRole('link', { name: note, exact: true }) });
}

async function addProject(page, title, domain) {
  const form = page.locator('details').filter({ has: text(page, 'Add a project') });
  if (await form.getAttribute('open') === null) await text(form, 'Add a project').press('Enter');
  await form.getByLabel('Project title').fill(title);
  if (domain) await form.getByRole('checkbox', { name: domain, exact: true }).check();
  await button(form, 'Save project').press('Enter');
  await visible(page.getByRole('heading', { name: title, exact: true }));
  return (await catalog(page)).projects.find((item) => item.title === title);
}

async function link(page, moment) {
  await page.getByLabel('Saved family moment').selectOption(moment.id);
  await button(page, 'Link moment to project').press('Enter');
  await visible(timelineRow(page, moment.note));
}

async function startAttempt(page, source) {
  const entry = timelineRow(page, source.note);
  await button(entry, 'Record as attempt').press('Enter');
  await entry.getByLabel('Event or activity').waitFor();
  return entry;
}

async function markSkill(page, moment, reason) {
  const section = page.getByRole('region', { name: 'Skill evidence' });
  const add = section.locator('details').filter({ has: text(page, 'Link an evidence moment') });
  if (await add.getAttribute('open') === null) await text(add, 'Link an evidence moment').press('Enter');
  await add.getByLabel('Evidence moment').selectOption(moment.id);
  await add.getByLabel('How does this show the skill?').fill(reason);
  await button(add, 'Save skill reason').press('Enter');
  await visible(text(section, reason));
}

async function inspectRestore(page, { earlier, revised, seed, math, exchange, testing, photoId, transferReason }) {
  const restoreDb = process.env.FIELDBOOK_TEST_RESTORE_DATABASE_URL;
  const restoreOrigin = process.env.FIELDBOOK_TEST_RESTORE_ORIGIN;
  const sourceMedia = process.env.FIELDBOOK_TEST_SOURCE_MEDIA;
  const restoreMedia = process.env.FIELDBOOK_TEST_RESTORE_MEDIA;
  const settings = [restoreDb, restoreOrigin, sourceMedia, restoreMedia];
  if (!settings.some(Boolean)) return { exercised: false };
  assert(settings.every(Boolean), 'Set all four restore fixture settings');
  for (const value of [restoreDb, restoreOrigin]) assert(['localhost', '127.0.0.1'].includes(new URL(value).hostname));
  assert.notEqual(restoreDb, database);
  assert.notEqual(new URL(restoreOrigin).origin, new URL(origin).origin);
  mkdirSync(restoreMedia, { recursive: true });
  assert.notEqual(realpathSync(restoreMedia), realpathSync(sourceMedia));
  assert.deepEqual(readdirSync(restoreMedia), []);
  assert.equal(sql(restoreDb, "SELECT to_regclass('public.moments') IS NULL"), 't');
  const staging = mkdtempSync(path.join(tmpdir(), 'fieldbook-b6-'));
  try {
    const snapshot = path.join(staging, 'fieldbook.dump');
    execFileSync('pg_dump', ['--dbname', database, '--format=custom', '--file', snapshot,
      ...['passkeys', 'sessions', 'enrollment_grants', 'webauthn_ceremonies'].map((table) =>
        `--exclude-table-data=public.${table}`),
    ]);
    chmodSync(snapshot, 0o600);
    const originals = sql(database, 'SELECT storage_key FROM assets WHERE storage_key IS NOT NULL ORDER BY storage_key');
    for (const key of originals.split('\n').filter(Boolean)) {
      assert.match(key, /^original-[0-9a-f-]{36}$/);
      copyFileSync(path.join(sourceMedia, key), path.join(restoreMedia, key));
    }
    execFileSync('pg_restore', ['--dbname', restoreDb, '--exit-on-error', '--single-transaction', snapshot]);
    assert.equal((await page.request.get(`${restoreOrigin}/readyz`)).status(), 200);
    assert.equal((await page.request.get(`${restoreOrigin}/api/v1/session`)).status(), 401);
    const restoredPage = await signIn('learner', restoreOrigin, restoreDb);
    await restoredPage.goto(`${restoreOrigin}/projects/${math.id}/`);
    await visible(timelineRow(restoredPage, revised.note));
    for (const source of [earlier, revised]) {
      await button(timelineRow(restoredPage, source.note), 'Compare this attempt').press('Enter');
    }
    const comparison = restoredPage.getByRole('region', { name: 'Attempt comparison' });
    await visible(text(comparison, 'Host, Participant'));
    await visible(text(comparison, 'A teammate distributed the strips.'));
    const restored = await api(restoredPage, `/moments/${revised.id}/attempt`, 'GET', undefined, restoreOrigin).then((r) => r.json());
    assert.deepEqual(restored.map(({ revision }) => revision), [3, 2, 1]);
    assert.equal(restored[0].details.prior_attempt_id, earlier.id);
    const restoredPhoto = await restoredPage.request.get(`${restoreOrigin}/media/${photoId}`);
    assert.equal(restoredPhoto.status(), 200);
    assert.equal(hash(await restoredPhoto.body()), asset.sha256);
    await restoredPage.goto(`${restoreOrigin}/skills/${testing.id}/`);
    await visible(text(restoredPage, `Fixture learner · ${transferReason}`));
    const transfer = (await api(restoredPage, `/skills/${testing.id}`, 'GET', undefined, restoreOrigin).then((r) => r.json())).transfers[0];
    assert.equal(transfer.to_project_id, exchange.id);
    assert.equal(transfer.to_moment_id, seed.id);
    return { exercised: true, revisions: restored.map(({ revision }) => revision), original_sha256: asset.sha256 };
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

try {
  const learner = await signIn('learner');
  const parent = await signIn('parent-one');
  const outsider = await signIn('other-learner');
  const guest = await signIn('relative');
  await button(parent, 'Projects').press('Enter');
  await text(parent, 'Household labels').press('Enter');
  for (const name of ['Mathematics', 'Neighborhood seed exchange']) {
    await parent.getByLabel('Label name').fill(name);
    await button(parent, 'Add label').press('Enter');
    await visible(text(parent.getByRole('list', { name: 'Household label list' }), name));
  }
  const labels = (await catalog(parent)).labels;
  const seedDomain = labels.find(({ name }) => name === 'Neighborhood seed exchange');
  const testing = labels.find(({ name }) => name === 'Test and improve');
  const explain = labels.find(({ name }) => name === 'Explain an idea');
  const baseline = fixture.moments.find(({ id }) => id === 'm1');
  const revision = fixture.moments.find(({ id }) => id === 'm2');
  const earlier = await createMoment(learner, 'The strips started at different edges.', baseline.activity_date);
  let revised = await createMoment(learner, 'I lined up the strips so the comparison was fair.', revision.activity_date);
  const seed = await createMoment(learner, 'I counted equal scoops before trading seeds.', '2026-02-04');
  const suggestion = await api(parent, `/moments/${seed.id}/comments`, 'POST', {
    submission_key: randomUUID(), kind: 'suggestion', body: 'Try sorting by size and compare again.',
  });
  assert.equal(suggestion.status(), 201);

  await button(learner, 'Projects').press('Enter');
  await learner.getByLabel('Choose a skill').selectOption(testing.id);
  await visible(text(learner, 'No demonstrated evidence yet.'));
  const math = await addProject(learner, 'Math club', 'Mathematics');
  await link(learner, revised);
  await link(learner, earlier);
  assert.deepEqual(await learner.getByRole('list', { name: 'Activity order' }).getByRole('link').allTextContents(),
    [earlier.note, revised.note]);

  const first = await startAttempt(learner, earlier);
  await first.getByLabel('Event or activity').fill('Practice comparison');
  await first.getByRole('checkbox', { name: 'Participant', exact: true }).check();
  await first.getByLabel('What happened').fill('The unequal edges caused an incorrect comparison.');
  await button(first, 'Save attempt').press('Enter');
  await visible(text(first, 'Attempt saved.'));
  await button(first, 'Close attempt editor').press('Enter');

  const next = await startAttempt(learner, revised);
  await next.getByLabel('Event or activity').fill('Host the fraction lesson');
  await next.getByRole('checkbox', { name: 'Participant', exact: true }).check();
  await next.getByRole('checkbox', { name: 'Host', exact: true }).check();
  await next.getByLabel('Prior attempt').selectOption(earlier.id);
  await next.getByLabel('My part', { exact: true }).fill('I lined up the strips.');
  await next.getByLabel('Others’ contributions').fill('Another student showed their comparison.');
  await next.getByLabel('Help I used').fill('A teammate distributed the strips.');
  await next.getByLabel('What happened').fill('The group could explain both starting points.');
  await button(next, 'Save attempt').press('Enter');
  await visible(text(next, 'Attempt saved.'));
  await next.getByLabel('Help I used').fill('A teammate distributed the printed set.');
  await button(next, 'Save attempt').press('Enter');
  await visible(text(next, 'Earlier attempt versions'));
  await text(next, 'Earlier attempt versions').press('Enter');
  await button(next, 'Restore attempt version 1').press('Enter');
  await visible(text(next, 'Attempt version restored.'));
  assert.equal(await next.getByLabel('Help I used').inputValue(), 'A teammate distributed the strips.');
  await button(next, 'Close attempt editor').press('Enter');

  const exchange = await addProject(learner, 'Neighborhood seed exchange', 'Neighborhood seed exchange');
  await link(learner, seed);
  const last = await startAttempt(learner, seed);
  await last.getByLabel('Event or activity').fill('Host the seed table');
  await last.getByRole('checkbox', { name: 'Host', exact: true }).check();
  await last.getByLabel('What happened').fill('Each bag contained the same number of seeds.');
  await last.getByLabel('How I checked').fill('I counted the seeds in both scoops.');
  await button(last, 'Save attempt').press('Enter');
  await visible(text(last, 'Attempt saved.'));
  await button(last, 'Compare this attempt').press('Enter');
  await learner.getByLabel('Choose a project').selectOption(math.id);
  await button(timelineRow(learner, revised.note), 'Compare this attempt').press('Enter');
  const compared = learner.getByRole('region', { name: 'Attempt comparison' });
  await visible(compared.getByRole('heading', { name: 'Host the seed table' }));
  await visible(compared.getByRole('heading', { name: 'Host the fraction lesson' }));
  assert.deepEqual(await compared.getByRole('heading', { level: 3 }).allTextContents(),
    ['Host the fraction lesson', 'Host the seed table']);
  await visible(text(compared, 'Host, Participant'));
  await visible(text(compared, 'A teammate distributed the strips.'));

  await learner.goto(`${origin}/?moment=${revised.id}`);
  await button(learner, 'Edit words').press('Enter');
  const edited = 'I aligned both starting points before comparing the strips.';
  await learner.getByLabel('Your words').fill(edited);
  await button(learner, 'Save changes').press('Enter');
  await visible(text(learner, 'Changes saved.'));
  revised = await api(learner, `/moments/${revised.id}`).then((r) => r.json());
  assert.equal(revised.note, edited);
  await parent.goto(`${origin}/?moment=${revised.id}`);
  const [chooser] = await Promise.all([parent.waitForEvent('filechooser'), button(parent, 'Choose a file').press('Enter')]);
  await chooser.setFiles({ name: 'practice-photo.jpg', mimeType: asset.mime,
    buffer: readFileSync(new URL(`../../fixtures/${asset.path}`, import.meta.url)) });
  await button(parent, 'Save and attach').press('Enter');
  await visible(parent.getByRole('link', { name: 'Download original for practice-photo.jpg' }));
  const [photo] = await api(learner, `/moments/${revised.id}/attachments`).then((r) => r.json());

  await button(learner, 'Projects').press('Enter');
  const unlabeled = await addProject(learner, 'Family practice');
  assert.deepEqual(unlabeled.domain_ids, []);
  await link(learner, revised);
  await button(timelineRow(learner, revised.note), 'Remove project connection').press('Enter');
  await visible(text(learner, 'No moments linked yet.'));
  assert.equal((await api(learner, `/moments/${revised.id}`)).status(), 200);
  assert.equal(hash(await learner.request.get(`/media/${photo.id}`).then((r) => r.body())), asset.sha256);
  assert((await api(learner, `/projects/${math.id}`).then((r) => r.json())).some(({ moment_id, withdrawn }) => moment_id === revised.id && !withdrawn));
  await text(learner, 'Removed project connections').press('Enter');
  await button(learner, 'Restore project connection').press('Enter');
  await visible(timelineRow(learner, revised.note));

  await learner.goto(`${origin}/skills/${testing.id}/`);
  const mathReason = 'I kept the starting edge the same in both trials.';
  const seedReason = 'I compared the scoops using the same cup.';
  await markSkill(learner, revised, mathReason);
  await markSkill(learner, seed, seedReason);
  await text(learner, 'Record a transfer').press('Enter');
  await learner.getByLabel('Earlier project and moment').selectOption(`${revised.id}/${math.id}`);
  await learner.getByLabel('New project and moment').selectOption(`${seed.id}/${exchange.id}`);
  const transferReason = 'I kept one condition the same so both results could be compared.';
  await learner.getByLabel('Why did this skill carry over?').fill(transferReason);
  await button(learner, 'Save transfer').press('Enter');
  await visible(text(learner, `Fixture learner · ${transferReason}`));
  const skill = await api(learner, `/skills/${testing.id}`).then((r) => r.json());
  const [transfer] = skill.transfers;
  await button(learner, 'Withdraw transfer').press('Enter');
  await visible(text(learner, 'Transfer withdrawn.'));
  await button(learner, 'Restore transfer').press('Enter');
  await visible(button(learner, 'Withdraw transfer'));
  await learner.getByRole('link', { name: 'Math club source', exact: true }).press('Enter');
  await visible(text(learner, edited));
  assert.equal(new URL(learner.url()).searchParams.get('moment'), revised.id);

  await learner.goto(`${origin}/skills/${explain.id}/`);
  const explainReason = 'I used the marked edges to explain the comparison.';
  await markSkill(learner, revised, explainReason);
  const examples = learner.getByRole('list', { name: 'Evidence examples' });
  assert.equal(await examples.getByRole('link', { name: edited }).getAttribute('href'), `/?moment=${revised.id}`);
  await button(examples, 'Remove skill connection').press('Enter');
  await visible(text(learner, 'No demonstrated evidence yet.'));
  await text(learner, 'Removed skill connections').press('Enter');
  await button(learner, 'Restore skill connection').press('Enter');
  await visible(text(examples, explainReason));
  assert.equal((await api(learner, `/skills/${testing.id}`).then((r) => r.json())).evidence.find((item) => item.moment_id === revised.id).reason, mathReason);

  await button(parent, 'Home').press('Enter');
  await button(parent, 'Projects').press('Enter');
  await text(parent, 'Household labels').press('Enter');
  await button(parent, 'Retire Neighborhood seed exchange').press('Enter');
  await visible(text(parent, 'Neighborhood seed exchange (retired)'));
  await parent.goto(`${origin}/projects/${exchange.id}/`);
  await visible(parent.getByRole('region', { name: 'Project timeline' }).getByText('Neighborhood seed exchange (retired)', { exact: true }));
  await text(parent, 'Add a project').press('Enter');
  assert.equal(await parent.getByRole('checkbox', { name: 'Neighborhood seed exchange' }).count(), 0);
  assert.equal((await api(learner, `/projects/${randomUUID()}`, 'PUT', { title: 'Retired choice', domain_ids: [seedDomain.id] })).status(), 422);

  for (const page of [outsider, guest]) {
    assert.equal((await api(page, `/projects/${math.id}`)).status(), 404);
    assert.equal((await api(page, `/skills/${testing.id}`)).status(), 404);
    assert.equal((await api(page, `/moments/${revised.id}/attempt`)).status(), 404);
  }
  assert.equal((await api(parent, `/moments/${revised.id}/attempt`, 'PUT', {
    expected_revision: 3, project_id: math.id, details: {},
  })).status(), 404);
  assert.equal(sql(database, `SELECT count(*) FROM moment_skills WHERE moment_id = '${revised.id}' AND withdrawn_at IS NULL`), '2');
  assert.equal(sql(database, `SELECT encode(sha256,'hex') FROM assets WHERE id = '${photo.id}'`), asset.sha256);

  const restore = await inspectRestore(learner, { earlier, revised, seed, math, exchange, testing, photoId: photo.id, transferReason });
  console.log(JSON.stringify({ compared: [revised.id, seed.id], revisions: 3, restore,
    fixture_ids: { household: households.A, earlier: earlier.id, revised: revised.id, seed: seed.id, math: math.id,
      exchange: exchange.id, unlabeled: unlabeled.id, testing: testing.id, explain: explain.id, transfer: transfer.id, photo: photo.id } }));
} finally {
  await browser.close();
}
