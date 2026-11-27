import assert from 'node:assert/strict';
import { randomUUID, createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { chromiumOptions, database, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const fixtures = JSON.parse(readFileSync(new URL('../../fixtures/assets.json', import.meta.url)));
const household = JSON.parse(readFileSync(new URL('../../fixtures/household.json', import.meta.url)));
const recording = household.external_links.find(({ id }) => id === 'recording-link');
const prefix = `archive-${randomUUID().slice(0, 8)}-`;
const { households, actors } = provision(prefix);
const browser = await chromium.launch(chromiumOptions);
const button = (page, name) => page.getByRole('button', { name, exact: true });
const text = (page, name) => page.getByText(name, { exact: true });
const visible = (locator) => locator.waitFor({ state: 'visible', timeout: 10000 });
const hash = (buffer) => createHash('sha256').update(buffer).digest('hex');
const bytes = (fixture) => readFileSync(new URL(`../../fixtures/${fixture.path}`, import.meta.url));
const sql = (query) => execFileSync('psql', [
  '--dbname', database, '-X', '-v', 'ON_ERROR_STOP=1', '-Atqc', query,
], { encoding: 'utf8' }).trim();

const blocked = [];
async function signIn(alias) {
  const context = await browser.newContext({ baseURL: origin, acceptDownloads: true });
  await context.route('**/*', (route) => {
    if (new URL(route.request().url()).origin === new URL(origin).origin) return route.continue();
    blocked.push(route.request().url());
    return route.abort('internetdisconnected');
  });
  const page = await context.newPage();
  await page.goto(origin);
  await virtualAuthenticator(page);
  await button(page, 'I have a passkey setup code').click();
  await page.getByLabel('Passkey setup code').fill(run('account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').click();
  await visible(text(page, 'Passkey added. Sign in with your account name.'));
  await page.getByLabel('Account name').fill(prefix + alias);
  await page.getByLabel('Account name').press('Enter');
  await visible(page.getByRole('heading', { name: 'Home', exact: true }));
  return page;
}

async function api(page, path, method = 'GET', body) {
  const session = await page.request.get('/api/v1/session').then((response) => response.json());
  return page.request.fetch(`/api/v1${path}`, {
    method, headers: { Origin: origin, 'X-CSRF-Token': session.csrf_token }, data: body,
  });
}

async function pick(page, control, name, mimeType, buffer) {
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), button(page, control).press('Enter')]);
  await chooser.setFiles({ name, mimeType, buffer });
}

async function download(page, label, digest) {
  const [file] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('link', { name: `Download original for ${label}`, exact: true }).press('Enter'),
  ]);
  assert.equal(hash(readFileSync(await file.path())), digest);
  return file.suggestedFilename();
}

function attached(page) {
  return page.getByRole('list', { name: 'Moment attachments' }).getByRole('listitem');
}

try {
  const parent = await signIn('parent-one');
  await button(parent, 'Write a sentence').click();
  await parent.getByLabel('What happened?').fill('Tournament practice and demos.');
  await button(parent, 'Save moment').click();
  await visible(text(parent, 'Moment saved.'));
  const [moment] = await api(parent, '/moments').then((response) => response.json());
  const attachments = `/moments/${moment.id}/attachments`;
  const read = () => api(parent, attachments).then((response) => response.json());
  const names = fixtures.map((fixture) => fixture.path.split('/').at(-1));

  for (const [index, fixture] of fixtures.entries()) {
    await pick(parent, 'Choose a file', names[index], fixture.mime, bytes(fixture));
    await button(parent, 'Save and attach').press('Enter');
    await visible(text(attached(parent), names[index]));
  }

  await button(parent, 'Paste a Drive link').press('Enter');
  await parent.getByLabel('Link label').fill(recording.label);
  await parent.getByLabel('HTTPS link').fill(recording.url);
  await button(parent, 'Save link').press('Enter');
  await visible(text(parent, 'Link saved.'));
  const fifth = attached(parent).nth(4);
  await visible(text(fifth, 'Family · Link only'));
  assert.equal(await fifth.getByRole('link', { name: 'Open source link' }).getAttribute('href'), recording.url);
  const before = await read();
  assert.deepEqual(before.map(({ label }) => label), [...names, recording.label]);
  assert.equal(before[4].copy_asset_id, null);
  const item = before[4].id;
  const copyPath = `${attachments}/${item}/copy`;
  assert.equal((await parent.request.get(`/media/${item}`)).status(), 404);

  for (const [index, fixture] of fixtures.entries()) {
    assert.equal(await download(parent, names[index], fixture.sha256), names[index]);
  }
  const media = `/media/${before[3].id}`;
  const partial = await parent.request.get(media, { headers: { Range: 'bytes=4-7' } });
  assert.equal(partial.status(), 206);
  assert.equal(partial.headers()['content-range'], `bytes 4-7/${fixtures[3].byte_length}`);
  assert.equal(partial.headers()['cache-control'], 'no-store, private');
  assert.equal(partial.headers()['x-content-type-options'], 'nosniff');
  assert.equal(partial.headers()['content-type'], 'application/octet-stream');
  assert.equal((await partial.body()).toString(), 'ftyp');
  assert.equal((await parent.request.get(media, { headers: { Range: 'bytes=99999-' } })).status(), 416);
  const head = await parent.request.head(media);
  assert.equal(head.status(), 200);
  assert.equal(Number(head.headers()['content-length']), fixtures[3].byte_length);

  const guest = await signIn('relative');
  const outsider = await signIn('other-learner');
  for (const page of [guest, outsider]) {
    const absent = await page.request.get(`/media/${randomUUID()}`);
    assert.equal(absent.status(), 404);
    for (const asset of before.slice(0, 2)) {
      const denied = await page.request.get(`/media/${asset.id}`, { headers: { Range: 'bytes=0-7' } });
      assert.equal(denied.status(), 404);
      assert.deepEqual(await denied.json(), await absent.json());
      assert.equal(denied.headers()['content-disposition'], undefined);
    }
  }
  await parent.bringToFront();

  // This passive MP4 free box makes the file span two browser hash chunks.
  const padding = Buffer.alloc(8 * 1024 * 1024);
  padding.writeUInt32BE(padding.length);
  padding.write('free', 4);
  const supplied = Buffer.concat([bytes(fixtures[3]), padding]);
  const expected = hash(supplied);
  const half = Math.floor(supplied.length / 2);
  const offsets = [];
  let uploadId;
  await parent.route('**/api/v1/uploads/*/parts/*', async (route) => {
    const segments = route.request().url().split('/');
    const offset = Number(segments.at(-1));
    uploadId = segments.at(-3);
    offsets.push(offset);
    if (offset === 0) {
      const result = await route.fetch({ postData: supplied.subarray(0, half) });
      assert.equal(result.status(), 200);
      await route.abort('connectionreset');
    } else {
      await route.continue();
    }
  });
  await pick(parent, 'Keep a copy', 'Tournament recording.mp4', 'video/mp4', supplied);
  await button(parent, 'Save copy').press('Enter');
  await visible(parent.getByRole('alert').filter({ hasText: 'Use Save to retry' }));
  assert.equal((await api(parent, `/uploads/${uploadId}`).then((response) => response.json())).received, half);
  assert.equal((await api(parent, copyPath, 'PUT', { original_id: uploadId })).status(), 404);
  await visible(text(fifth, 'Family · Link only'));
  assert.equal((await parent.request.get(`/media/${item}`)).status(), 404);

  await button(parent, 'Save copy').press('Enter');
  await visible(text(fifth, 'Family · Copy saved'));
  assert.deepEqual(offsets, [0, half]);
  const archived = await read();
  assert.deepEqual(archived.map(({ id }) => id), before.map(({ id }) => id));
  assert.equal(archived[4].copy_asset_id, uploadId);
  assert.equal(archived[4].source_url, recording.url);
  assert.equal(await download(parent, recording.label, expected), 'Tournament recording.mp4');
  assert.equal((await parent.request.get(`/media/${uploadId}`)).status(), 404);
  const replays = await Promise.all(Array.from({ length: 2 }, () => api(parent, copyPath, 'PUT', { original_id: uploadId })));
  assert.deepEqual(replays.map((response) => response.status()), [204, 204]);
  assert.equal((await api(parent, copyPath, 'PUT', { original_id: before[3].id })).status(), 409);
  assert.equal(sql(`SELECT count(*) FROM assets WHERE household_id = '${households.A}' AND sha256 = decode('${expected}', 'hex')`), '1');

  await button(parent, 'Withdraw copy').press('Enter');
  await visible(text(fifth, 'Family · Link only'));
  assert.equal((await parent.request.get(`/media/${item}`)).status(), 404);
  await button(parent, 'Restore copy').press('Enter');
  await visible(text(fifth, 'Family · Copy saved'));
  assert.equal((await read())[4].copy_asset_id, uploadId);

  const submission = randomUUID();
  const later = { label: 'Another reference', source_url: 'https://example.invalid/another', submission_key: submission };
  for (const source_url of ['http://example.invalid/', 'javascript:alert(1)', 'https://name:pass@example.invalid/']) {
    assert.equal((await api(parent, '/assets/links', 'POST', { ...later, source_url })).status(), 422);
  }
  const first = await api(parent, '/assets/links', 'POST', later);
  assert.equal(first.status(), 201);
  const { id: after } = await first.json();
  assert.deepEqual(await api(parent, '/assets/links', 'POST', later).then((response) => response.json()), { id: after });
  assert.equal((await api(parent, '/assets/links', 'POST', { ...later, label: 'Changed' })).status(), 409);
  assert.equal((await api(parent, `${attachments}/${after}`, 'PUT')).status(), 204);

  await button(fifth, 'Remove link').press('Enter');
  await visible(text(parent, 'Link removed.'));
  assert.equal((await parent.request.get(`/media/${item}`)).status(), 404);
  await button(parent, 'Restore link').press('Enter');
  await visible(text(fifth, 'Family · Copy saved'));
  assert.deepEqual((await read()).map(({ id }) => id), [...before.map(({ id }) => id), after]);
  assert.equal((await api(parent, `${attachments}/${after}`, 'DELETE')).status(), 204);
  assert.deepEqual(blocked, []);
  console.log('B3.3 fixture', {
    household: households.A, guardian: actors['parent-one'], guest: actors.relative, moment: moment.id,
    ordered: before.map(({ id }) => id), archived: uploadId, sha256: expected, bytes: supplied.length,
    resumeOffset: half, browser: browser.version(),
  });
} finally {
  await browser.close();
}
