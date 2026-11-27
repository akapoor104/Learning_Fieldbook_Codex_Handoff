import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { chromiumOptions, database, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const fixture = JSON.parse(readFileSync(new URL('../../fixtures/household.json', import.meta.url), 'utf8'));
const entry = fixture.moments.find(({ visibility }) => visibility === 'diary');
const prefix = `diary-${randomUUID().slice(0, 8)}-`;
const { households, actors } = provision(prefix);
const learnerId = actors[entry.learner];
const diaryId = randomUUID();
const filename = `${entry.title}.jpg`;
const photo = readFileSync(new URL('../../fixtures/media/practice-photo.jpg', import.meta.url));
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const psql = (query) => ['--dbname', database, '-X', '-v', 'ON_ERROR_STOP=1', '-Atqc', query];
const sql = (query) => execFileSync('psql', psql(query), { encoding: 'utf8' }).trim();
const button = (page, name) => page.getByRole('button', { name, exact: true });
const browser = await chromium.launch(chromiumOptions);

async function signIn(alias) {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(origin);
  await virtualAuthenticator(page);
  await button(page, 'I have a passkey setup code').click();
  await page.getByLabel('Passkey setup code').fill(run('account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').click();
  await page.getByText('Passkey added. Sign in with your account name.').waitFor();
  await page.getByLabel('Account name').fill(prefix + alias);
  await page.getByLabel('Account name').press('Enter');
  await page.getByRole('heading', { name: 'Home', exact: true }).waitFor();
  return page;
}

async function write(page, method, path, data, status) {
  const session = await page.request.get(`${origin}/api/v1/session`);
  const { csrf_token: token } = await session.json();
  const headers = { Origin: origin, 'X-CSRF-Token': token };
  if (Buffer.isBuffer(data)) headers['Content-Type'] = 'application/octet-stream';
  const response = await page.request.fetch(`${origin}/api/v1${path}`, { method, data, headers });
  assert.equal(response.status(), status, path);
  return status === 204 ? undefined : response.json();
}

async function get(page, path, status, method = 'GET', headers = {}) {
  const response = await page.request.fetch(origin + path, { method, headers });
  assert.equal(response.status(), status, `${method} ${path}`);
  assert.equal(response.headers()['cache-control'], 'no-store, private');
  if (status === 404) {
    assert.equal(response.headers()['content-disposition'], undefined);
    if (method === 'GET') assert.deepEqual(await response.json(), { error: 'not_found' });
  }
  return response;
}

try {
  const learner = await signIn(entry.learner);
  const family = await write(learner, 'POST', '/moments', {
    submission_key: randomUUID(), note: 'An ordinary family sentence.',
  }, 201);
  const source = await write(learner, 'POST', '/assets/links', {
    submission_key: randomUUID(), label: 'Fictional practice link',
    source_url: 'https://example.invalid/practice',
  }, 201);
  const file = await write(learner, 'POST', '/uploads', {
    submission_key: randomUUID(), label: filename, media_type: 'image/jpeg',
    byte_count: photo.length, sha256: digest(photo),
  }, 201);
  await write(learner, 'PUT', `/uploads/${file.id}/parts/0`, photo, 200);
  await write(learner, 'POST', `/uploads/${file.id}/finalize`, undefined, 200);
  const familyPath = `/api/v1/moments/${family.id}`;
  const diaryPath = `/api/v1/diary/${diaryId}`;
  const diaryMedia = `/diary/media/${file.id}`;

  // Only the synthetic seed uses the operator DB path. The family UI never
  // accepts a diary submission, and its upload manifest must not retain it.
  sql(`BEGIN;
    DELETE FROM private_uploads WHERE asset_id = '${file.id}';
    UPDATE assets SET scope_owner = '${learnerId}' WHERE id = '${file.id}';
    INSERT INTO moments (id, household_id, learner_id, recorder_id, submission_key, submission_hash, voice, visibility)
      VALUES ('${diaryId}', '${households.A}', '${learnerId}', '${learnerId}', '${randomUUID()}',
        decode(repeat('00', 32), 'hex'), 'student', 'diary');
    INSERT INTO moment_revisions (moment_id, household_id, revision, editor_id, note, activity_date)
      VALUES ('${diaryId}', '${households.A}', 1, '${learnerId}', '${entry.title}', '${entry.activity_date}');
    INSERT INTO moment_assets (moment_id, household_id, asset_id, position, scope_owner)
      VALUES ('${diaryId}', '${households.A}', '${file.id}', 0, '${learnerId}');
    COMMIT;`);

  const saved = await (await get(learner, diaryPath, 200)).json();
  assert.equal(saved.note, entry.title);
  assert.equal(saved.visibility, 'diary');
  assert.deepEqual(saved.asset_ids, [file.id]);
  await get(learner, `/api/v1/diary/${family.id}`, 404);
  await button(learner, 'My diary').click();
  await learner.getByText(entry.title, { exact: true }).waitFor();
  const [download] = await Promise.all([
    learner.waitForEvent('download'), learner.getByRole('link', { name: 'Download attachment 1' }).click(),
  ]);
  assert.equal(download.suggestedFilename(), filename);
  assert.equal(digest(readFileSync(await download.path())), digest(photo));
  const range = await get(learner, diaryMedia, 206, 'GET', { Range: 'bytes=0-15' });
  assert.deepEqual(await range.body(), photo.subarray(0, 16));
  assert((await get(learner, diaryMedia, 200, 'HEAD')).headers()['content-disposition'].includes(filename));
  await learner.goto(`${origin}/diary/`);
  await learner.getByText(entry.title, { exact: true }).waitFor();
  await button(learner, 'Home').click();
  await button(learner, 'Family journal').click();
  await learner.getByText(family.note, { exact: true }).waitFor();
  assert.equal(await learner.getByText(entry.title, { exact: true }).count(), 0);

  const sameHousehold = 'second-learner';
  actors[sameHousehold] = run('account', 'add', households.A, prefix + sameHousehold,
    'Fixture second learner', 'learner');
  const others = [...fixture.actors, { id: sameHousehold, role: 'learner', household: 'A' }];
  const otherPages = await Promise.all(others.filter(({ id }) => id !== entry.learner)
    .map(async ({ id, role, household }) => ({ id, role, household, page: await signIn(id) })));
  for (const { id, role, household, page } of otherPages) {
    const canSeeFamily = role !== 'guest';
    const list = await (await get(page, '/api/v1/moments', canSeeFamily ? 200 : 404)).json();
    if (canSeeFamily) assert.deepEqual(list.map(({ id }) => id), household === 'A' ? [family.id] : []);
    if (id === 'parent-one') {
      await button(page, 'Verify it’s you').click();
      await page.getByText('Identity verified.', { exact: true }).waitFor();
    }
    await get(page, diaryPath, 404);
    await get(page, diaryMedia, 404, 'HEAD');
    await get(page, diaryMedia, 404, 'GET', { Range: 'bytes=0-15' });
    const diaryList = await get(page, '/api/v1/diary', role === 'learner' ? 200 : 404);
    if (role === 'learner') assert.deepEqual(await diaryList.json(), []);
    if (role !== 'learner') assert.equal(await button(page, 'My diary').count(), 0);
    if (role === 'guardian' && household === 'A') {
      const inbox = await get(page, `/api/v1/family/photo-inbox/${learnerId}`, 200);
      assert.deepEqual(await inbox.json(), []);
    }
  }

  for (const page of [learner, ...otherPages.map(({ page }) => page)]) {
    for (const suffix of ['', '/revisions', '/attachments', '/comments']) {
      await get(page, `/api/v1/moments/${diaryId}${suffix}`, 404);
    }
    await get(page, `/media/${file.id}`, 404, 'HEAD');
    await get(page, `/api/v1/uploads/${file.id}`, 404);
  }
  await write(learner, 'POST', '/moments', {
    submission_key: randomUUID(), note: 'Attempted link into family.', asset_ids: [file.id],
  }, 404);
  await write(learner, 'PATCH', `/moments/${diaryId}`, {
    expected_revision: 1, note: 'An attempted family edit.',
  }, 404);
  await write(learner, 'PUT', `/moments/${family.id}/attachments/${source.id}`, undefined, 204);
  await write(learner, 'PUT', `/moments/${family.id}/attachments/${source.id}/copy`,
    { original_id: file.id }, 404);
  await get(learner, `/media/${source.id}`, 404);
  assert.deepEqual((await (await get(learner, familyPath, 200)).json()).asset_ids, [source.id]);

  const wrongScope = (query) => {
    const result = spawnSync('psql', psql(query), { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /violates foreign key constraint/);
  };
  wrongScope(`INSERT INTO moment_assets (moment_id, household_id, asset_id, position)
      VALUES ('${family.id}', '${households.A}', '${file.id}', 1)`);
  wrongScope(`INSERT INTO family_photo_inbox (asset_id, household_id, learner_id)
      VALUES ('${file.id}', '${households.A}', '${learnerId}')`);
  wrongScope(`UPDATE assets SET copy_asset_id = '${file.id}' WHERE id = '${source.id}'`);

  // Restricted selectors also cover future read surfaces built on the family views.
  assert.equal(sql(`SELECT count(*) FROM family_moments WHERE household_id = '${households.A}'`), '1');
  assert.equal(sql(`SELECT count(*) FROM family_moments m JOIN moment_revisions r ON r.moment_id = m.id
    WHERE m.household_id = '${households.A}' AND r.note LIKE '%${entry.title}%'`), '0');
  assert.equal(sql(`SELECT count(*) FROM family_assets WHERE household_id = '${households.A}'
    AND label LIKE '%${entry.title}%'`), '0');
  assert.equal(sql(`SELECT encode(sha256, 'hex') FROM assets WHERE id = '${file.id}'`), digest(photo));
  console.log(JSON.stringify({
    fixture: entry.id, diaryId, fileId: file.id, familyId: family.id, sourceId: source.id,
    sha256: digest(photo), ordinaryActorsChecked: otherPages.map(({ id }) => id),
  }));
} finally {
  await browser.close();
}
