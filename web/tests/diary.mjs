import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { chromiumOptions, database, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const fixture = JSON.parse(readFileSync(new URL('../../fixtures/household.json', import.meta.url), 'utf8'));
const entry = fixture.moments.find(({ visibility }) => visibility === 'diary');
const parentAccess = process.argv.includes('--parent-access');
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

const accessRoute = '/api/v1/diary/parent-access';
const historyRoute = '/api/v1/diary/access-history';

async function session(page) {
  return (await get(page, '/api/v1/session', 200)).json();
}

async function post(page, route, csrf, data, cookie) {
  const headers = { Origin: origin, 'X-CSRF-Token': csrf };
  if (cookie) headers.Cookie = cookie;
  return page.request.post(origin + route, { headers, data });
}

async function assertion(page, csrf, kind) {
  const start = await post(page, accessRoute + '/start', csrf, { learner_id: learnerId, kind });
  assert.equal(start.status(), 200);
  const { ceremony, options } = await start.json();
  const credential = await page.evaluate(async (data) => {
    const publicKey = PublicKeyCredential.parseRequestOptionsFromJSON(data);
    return (await navigator.credentials.get({ publicKey })).toJSON();
  }, options.publicKey);
  return { ceremony, credential };
}

async function parentJourney(learner, otherPages, diaryPath, diaryMedia, family) {
  const one = otherPages.find(({ id }) => id === 'parent-one').page;
  const two = otherPages.find(({ id }) => id === 'parent-two').page;
  const outsider = otherPages.find(({ id }) => id === 'other-parent').page;
  const relative = otherPages.find(({ id }) => id === 'relative').page;
  const sibling = otherPages.find(({ id }) => id === 'second-learner').page;
  const oneBefore = await session(one);
  const twoBefore = await session(two);
  const siblingId = actors['second-learner'];
  const siblingDiary = randomUUID();
  const household = households.A;
  sql(`BEGIN;
    INSERT INTO moments (id, household_id, learner_id, recorder_id, submission_key, submission_hash, voice, visibility)
      VALUES ('${siblingDiary}', '${household}', '${siblingId}', '${siblingId}', '${randomUUID()}',
        decode(repeat('00', 32), 'hex'), 'student', 'diary');
    INSERT INTO moment_revisions (moment_id, household_id, revision, editor_id, note)
      VALUES ('${siblingDiary}', '${household}', 1, '${siblingId}', 'A second learner’s diary example.');
    COMMIT;`);
  assert.equal((await post(one, accessRoute + '/start', 'invalid',
    { learner_id: learnerId, kind: 'view' })).status(), 403);
  const firstProof = await assertion(one, oneBefore.csrf_token, 'view');
  assert.equal((await post(one, accessRoute + '/finish', 'invalid', firstProof)).status(), 403);
  const count = () => sql(`SELECT count(*) FROM diary_access_events WHERE household_id = '${household}'`);
  const firstCookie = (await one.context().cookies())[0];
  assert(firstCookie);
  const firstCookieHeader = `${firstCookie.name}=${firstCookie.value}`;

  assert.equal((await post(one, '/api/v1/auth/step-up/finish', oneBefore.csrf_token, firstProof)).status(), 401);
  assert.equal((await post(two, accessRoute + '/finish', twoBefore.csrf_token, firstProof)).status(), 401);
  for (const [page, target, status] of [[one, actors['other-learner'], 404],
    [outsider, learnerId, 404], [relative, learnerId, 403], [learner, learnerId, 403]]) {
    assert.equal((await post(page, accessRoute + '/start', (await session(page)).csrf_token,
      { learner_id: target, kind: 'view' })).status(), status);
  }
  assert.deepEqual(await (await get(learner, historyRoute, 200)).json(), []);

  // Fail audit insertion after the assertion is verified and rotation has started.
  sql(`CREATE FUNCTION deny_test_diary_access() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.guardian_id = '${actors['parent-one']}' THEN RAISE EXCEPTION 'test audit storage failure'; END IF;
        RETURN NEW;
      END; $$;
      CREATE TRIGGER deny_test_diary_access BEFORE INSERT ON diary_access_events
        FOR EACH ROW EXECUTE FUNCTION deny_test_diary_access();`);
  try {
    const failed = await post(one, accessRoute + '/finish', oneBefore.csrf_token, firstProof);
    assert.equal(failed.status(), 503);
    assert.equal(failed.headers()['set-cookie'], undefined);
    assert.deepEqual(await failed.json(), { error: 'unavailable' });
  } finally {
    sql('DROP TRIGGER deny_test_diary_access ON diary_access_events; DROP FUNCTION deny_test_diary_access();');
  }
  assert.equal(count(), '0');
  const afterFailure = await session(one);
  assert.equal(afterFailure.csrf_token, oneBefore.csrf_token);
  assert.equal(afterFailure.diary_learner_id, null);
  await get(one, '/api/v1/moments', 200);
  await get(one, diaryPath, 404);
  await get(one, diaryMedia, 404, 'HEAD');
  assert.equal((await post(one, accessRoute + '/finish', oneBefore.csrf_token, firstProof)).status(), 401);

  const retry = await assertion(one, oneBefore.csrf_token, 'view');
  const finished = await Promise.all([1, 2].map(() =>
    post(one, accessRoute + '/finish', oneBefore.csrf_token, retry, firstCookieHeader)));
  assert.deepEqual(finished.map((response) => response.status()).sort(), [200, 401]);
  const elevated = await finished.find((response) => response.ok()).json();
  assert.notEqual(elevated.csrf_token, oneBefore.csrf_token);
  assert.equal(elevated.diary_learner_id, learnerId);
  assert(elevated.diary_seconds > 0 && elevated.diary_seconds <= 300);
  assert.equal(elevated.elevated_seconds, 0);
  assert.equal(count(), '1');
  assert.equal((await one.request.get(origin + diaryPath, { headers: { Cookie: firstCookieHeader } })).status(), 401);
  assert.equal((await (await get(one, diaryPath, 200)).json()).asset_ids.length, 1);
  assert.deepEqual((await (await get(one, '/api/v1/diary', 200)).json()).map(({ id }) => id), [diaryId]);
  const bytes = await get(one, diaryMedia, 206, 'GET', { Range: 'bytes=0-15' });
  assert.deepEqual(await bytes.body(), photo.subarray(0, 16));
  await get(one, diaryMedia, 200, 'HEAD');
  await get(one, '/api/v1/moments', 404);
  await get(one, '/api/v1/diary/' + family.id, 404);
  await get(one, '/api/v1/diary/' + siblingDiary, 404);
  await get(one, historyRoute, 404);
  assert.equal((await one.request.patch(origin + diaryPath, {
    headers: { Origin: origin, 'X-CSRF-Token': elevated.csrf_token },
    data: { expected_revision: 1, note: 'Attempted diary edit' },
  })).status(), 405);
  await write(one, 'POST', '/moments', { submission_key: randomUUID(), note: 'Diary mode attempt' }, 404);
  await write(one, 'POST', '/assets/links', {
    submission_key: randomUUID(), label: 'Diary mode attempt', source_url: 'https://example.invalid',
  }, 404);
  assert.equal((await post(one, '/api/v1/auth/step-up/start', elevated.csrf_token)).status(), 403);
  assert.equal((await post(one, accessRoute + '/start', elevated.csrf_token,
    { learner_id: learnerId, kind: 'view' })).status(), 403);
  await get(two, diaryPath, 404);
  const rotatedCookie = (await one.context().cookies())[0];
  assert(rotatedCookie);
  const rotatedCookieHeader = `${rotatedCookie.name}=${rotatedCookie.value}`;
  assert.equal((await one.request.delete(origin + '/api/v1/session', {
    headers: { Origin: origin, Cookie: firstCookieHeader, 'X-CSRF-Token': 'invalid' },
  })).status(), 403);
  const logout = await one.request.delete(origin + '/api/v1/session', {
    headers: { Origin: origin, Cookie: firstCookieHeader, 'X-CSRF-Token': oneBefore.csrf_token },
  });
  assert.equal(logout.status(), 204);
  assert.equal((await one.request.get(origin + diaryPath, { headers: { Cookie: rotatedCookieHeader } })).status(), 401);
  await get(one, diaryPath, 401);
  await get(one, diaryMedia, 401, 'HEAD');
  assert.equal(count(), '1');

  // Expedite only this fixture’s expiry, leaving the normal session lifetime intact.
  const recovery = await assertion(two, twoBefore.csrf_token, 'recovery');
  sql(`CREATE FUNCTION shorten_test_diary_access() RETURNS trigger LANGUAGE plpgsql AS $$
      BEGIN
        IF NEW.guardian_id = '${actors['parent-two']}' THEN
          NEW.expires_at := NEW.recorded_at + interval '8 seconds';
        END IF;
        RETURN NEW;
      END; $$;
      CREATE TRIGGER shorten_test_diary_access BEFORE INSERT ON diary_access_events
        FOR EACH ROW EXECUTE FUNCTION shorten_test_diary_access();`);
  try {
    const result = await post(two, accessRoute + '/finish', twoBefore.csrf_token, recovery);
    assert.equal(result.status(), 200);
    assert.notEqual((await result.json()).csrf_token, twoBefore.csrf_token);
  } finally {
    sql('DROP TRIGGER shorten_test_diary_access ON diary_access_events; DROP FUNCTION shorten_test_diary_access();');
  }
  assert.equal((await (await get(two, diaryPath, 200)).json()).note, entry.title);
  const download = await get(two, diaryMedia, 200);
  assert.equal(digest(await download.body()), digest(photo));
  const refreshed = await write(two, 'POST', '/session/keep-alive', undefined, 200);
  assert(refreshed.diary_seconds <= 8);
  const deadline = Date.now() + 12000;
  while ((await session(two)).diary_seconds > 0 && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  assert.equal((await session(two)).diary_learner_id, null);
  await get(two, diaryPath, 404);
  await get(two, diaryMedia, 404, 'HEAD');
  await get(two, diaryMedia, 404, 'GET', { Range: 'bytes=0-15' });
  assert.deepEqual((await (await get(two, '/api/v1/moments', 200)).json()).map(({ id }) => id), [family.id]);

  await learner.reload();
  const history = await (await get(learner, historyRoute, 200)).json();
  assert.equal(count(), '2');
  assert.deepEqual(history.map(({ guardian_id, kind }) => [guardian_id, kind]), [
    [actors['parent-two'], 'recovery'], [actors['parent-one'], 'view'],
  ]);
  for (const event of history) {
    assert(event.guardian_name.startsWith('Fixture parent-'));
    assert(Date.parse(event.recorded_at) < Date.parse(event.expires_at));
    assert(Date.parse(event.expires_at) - Date.parse(event.recorded_at) <= 300000);
  }
  assert.deepEqual(await (await get(sibling, historyRoute, 200)).json(), []);
  for (const query of [
    `UPDATE diary_access_events SET kind='view' WHERE id='${history[0].id}'`,
    `DELETE FROM diary_access_events WHERE id='${history[0].id}'`,
    'TRUNCATE diary_access_events CASCADE',
  ]) {
    const result = spawnSync('psql', psql(query), { encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /append-only/);
  }
  assert.deepEqual(await (await get(learner, historyRoute, 200)).json(), history);
  return history;
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
  const accessEvents = parentAccess ? await parentJourney(learner, otherPages, diaryPath, diaryMedia, family) : [];
  console.log(JSON.stringify({
    accessEvents,
    fixture: entry.id, diaryId, fileId: file.id, familyId: family.id, sourceId: source.id,
    sha256: digest(photo), ordinaryActorsChecked: otherPages.map(({ id }) => id),
  }));
} finally {
  await browser.close();
}
