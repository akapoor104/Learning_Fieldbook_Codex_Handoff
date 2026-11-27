import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { chromiumOptions, database, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const caseMode = process.argv.includes('--case');
if (caseMode) {
  const request = JSON.parse(readFileSync(0, 'utf8'));
  assert.equal(request.case, 'guest-revoked');
  assert.equal(request.input.fixture, 'household');
}
const fixture = JSON.parse(readFileSync(new URL('../../fixtures/household.json', import.meta.url)));
const assets = JSON.parse(readFileSync(new URL('../../fixtures/assets.json', import.meta.url)));
const prefix = `share-${randomUUID().slice(0, 8)}-`;
const { households, actors } = provision(prefix);
actors['other-relative'] = run('account', 'add', households.A, prefix + 'other-relative', 'Another relative', 'guest');

const browser = await chromium.launch(chromiumOptions);
const button = (page, name) => page.getByRole('button', { name, exact: true });
const visible = (target) => target.waitFor({ state: 'visible', timeout: 10000 });
const text = (page, name) => page.getByText(name, { exact: true });
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const sql = (query) => execFileSync('psql', ['--dbname', database, '-X', '-v', 'ON_ERROR_STOP=1', '-Atqc', query],
  { encoding: 'utf8' }).trim();

async function signIn(alias) {
  const context = await browser.newContext({ baseURL: origin, acceptDownloads: true, viewport: { width: 360, height: 800 } });
  const page = await context.newPage();
  await page.goto(origin);
  await virtualAuthenticator(page);
  await button(page, 'I have a passkey setup code').click();
  await page.getByLabel('Passkey setup code').fill(run('account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').press('Enter');
  await visible(text(page, 'Passkey added. Sign in with your account name.'));
  await page.getByLabel('Account name').fill(prefix + alias);
  await page.getByLabel('Account name').press('Enter');
  await visible(page.getByRole('heading', { name: 'Home', exact: true }));
  return page;
}

async function api(page, path, method = 'GET', body) {
  const session = await page.request.get('/api/v1/session').then((response) => response.json());
  return page.request.fetch(`/api/v1${path}`, {
    method, headers: { Origin: origin, 'X-CSRF-Token': session.csrf_token, ...(Buffer.isBuffer(body) ? { 'Content-Type': 'application/octet-stream' } : {}) }, data: body,
  });
}

async function attach(page, fixtureId, filename) {
  const asset = assets.find(({ id }) => id === fixtureId);
  const name = filename ?? asset.path.split('/').at(-1);
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), button(page, 'Choose a file').press('Enter')]);
  await chooser.setFiles({ name, mimeType: asset.mime, buffer: readFileSync(new URL(`../../fixtures/${asset.path}`, import.meta.url)) });
  await button(page, 'Save and attach').press('Enter');
  await visible(text(page.getByRole('list', { name: 'Moment attachments' }), name));
}

async function review(page) {
  await text(page, 'Share with a relative').press('Enter');
  try {
    await page.getByLabel('Relative', { exact: true }).selectOption(actors.relative, { timeout: 8000 });
  } catch (failure) {
    console.error(await page.locator('main').innerText());
    throw failure;
  }
  await page.getByLabel('Share these words', { exact: true }).check();
  await page.getByLabel('practice-photo.jpg', { exact: true }).check();
  await button(page, 'Preview as relative').press('Enter');
  const preview = page.getByRole('region', { name: 'Guest preview' });
  await visible(text(preview, fixture.highlights[0].visible_text));
  assert.equal(await text(preview, 'teacher-note.pdf').count(), 0);
  return preview;
}

try {
  const parent = await signIn('parent-one');
  const learner = await signIn('learner');
  const guest = await signIn('relative');
  const other = await signIn('other-relative');
  const note = fixture.highlights[0].visible_text;
  const set = await api(learner, '/moments', 'POST', { note, submission_key: randomUUID() });
  assert.equal(set.status(), 201);
  const moment = await set.json();
  const path = `/moments/${moment.id}`;
  const diaryEntry = fixture.moments.find(({ visibility }) => visibility === 'diary');
  const diaryId = randomUUID();
  const diaryPhoto = readFileSync(new URL('../../fixtures/media/practice-photo.jpg', import.meta.url));
  const upload = await api(learner, '/uploads', 'POST', {
    submission_key: randomUUID(), label: 'private-diary.jpg', media_type: 'image/jpeg',
    byte_count: diaryPhoto.length, sha256: hash(diaryPhoto),
  }).then((r) => r.json());
  assert.equal((await api(learner, `/uploads/${upload.id}/parts/0`, 'PUT', diaryPhoto)).status(), 200);
  const { asset_id: diaryAsset } = await api(learner, `/uploads/${upload.id}/finalize`, 'POST').then((r) => r.json());
  sql(`BEGIN;
    DELETE FROM private_uploads WHERE asset_id = '${diaryAsset}';
    UPDATE assets SET scope_owner = '${actors.learner}' WHERE id = '${diaryAsset}';
    INSERT INTO moments (id, household_id, learner_id, recorder_id, submission_key, submission_hash, voice, visibility)
      VALUES ('${diaryId}', '${households.A}', '${actors.learner}', '${actors.learner}', '${randomUUID()}',
        decode(repeat('00', 32), 'hex'), 'student', 'diary');
    INSERT INTO moment_revisions (moment_id, household_id, revision, editor_id, note)
      VALUES ('${diaryId}', '${households.A}', 1, '${actors.learner}', '${diaryEntry.title}');
    INSERT INTO moment_assets (moment_id, household_id, asset_id, position, scope_owner)
      VALUES ('${diaryId}', '${households.A}', '${diaryAsset}', 0, '${actors.learner}');
    COMMIT;`);
  assert.equal((await learner.request.get(`/diary/media/${diaryAsset}`)).status(), 200);
  const otherMoment = await api(learner, '/moments', 'POST', { note: 'Another family entry.', submission_key: randomUUID() }).then((r) => r.json());
  await parent.goto(`${origin}/?moment=${moment.id}`);
  await attach(parent, 'photo');
  await attach(parent, 'letter');
  const [photo, letter] = await api(parent, `${path}/attachments`).then((r) => r.json());
  assert.equal(photo.label, 'practice-photo.jpg');
  assert.equal(letter.label, 'teacher-note.pdf');

  await text(parent, 'Record teacher feedback').press('Enter');
  await parent.getByLabel('Teacher’s name').fill('Fictional teacher');
  await parent.getByLabel('Feedback date').fill('2026-01-14');
  await parent.getByLabel('Attached teacher PDF').selectOption(letter.id);
  await button(parent, 'Save teacher feedback').press('Enter');
  await visible(parent.getByText('Written by Fictional teacher · Uploaded by Fixture parent-one', { exact: false }));
  await parent.getByLabel('Teacher’s name').fill('Fictional teacher');
  await parent.getByLabel('Your paraphrase').fill('They noticed the second attempt.');
  await button(parent, 'Save teacher feedback').press('Enter');
  await visible(text(parent, 'Fictional teacher’s observation · Paraphrased by Fixture parent-one'));
  const privateComment = await api(parent, `${path}/comments`, 'POST', { submission_key: randomUUID(), body: 'Household-only comment' });
  assert.equal(privateComment.status(), 201);

  await learner.goto(`${origin}/?moment=${moment.id}`);
  const learnerPreview = await review(learner);
  assert.equal(await button(learnerPreview, 'Create invitation').count(), 0);
  await review(parent);
  await button(parent, 'Create invitation').press('Enter');
  await visible(parent.getByLabel('Code to give this relative'));
  const code = await parent.getByLabel('Code to give this relative').inputValue();
  const [id, token] = code.split('.');
  const share = `/shares/${id}`;
  assert.equal((await api(other, `${share}/accept`, 'POST', { token })).status(), 404);
  await guest.getByLabel('Relative invitation code').fill(code);
  await button(guest, 'Accept invitation').press('Enter');
  await visible(text(guest, note));
  const photoPath = `${share}/media/${photo.id}`;
  const letterPath = `${share}/media/${letter.id}`;
  const before = {
    photo: (await guest.request.get(photoPath)).status(),
    letter: (await guest.request.get(letterPath)).status(),
  };
  assert.deepEqual(before, { photo: 200, letter: 404 });
  const [download] = await Promise.all([
    guest.waitForEvent('download'),
    guest.getByRole('link', { name: photo.label, exact: true }).press('Enter'),
  ]);
  const digest = assets.find(({ id }) => id === 'photo').sha256;
  assert.equal(hash(readFileSync(await download.path())), digest);
  assert.equal((await guest.request.get(`/diary/media/${diaryAsset}`)).status(), 404);
  assert.equal((await guest.request.get(`/media/${diaryAsset}`)).status(), 404);
  assert.equal((await api(guest, `/moments/${otherMoment.id}`)).status(), 404);
  assert.equal((await api(other, `${share}/comments`)).status(), 404);

  const reply = fixture.comments.find(({ id }) => id === 'cr1').text;
  await guest.getByLabel('Comment on this share').fill(reply);
  await button(guest, 'Post shared comment').press('Enter');
  await visible(text(guest, reply));
  await button(guest, 'Withdraw shared comment').press('Enter');
  await visible(button(guest, 'Restore shared comment'));
  await button(guest, 'Restore shared comment').press('Enter');
  await visible(button(guest, 'Withdraw shared comment'));
  assert.equal(await text(guest, 'Household-only comment').count(), 0);
  assert.equal(await guest.getByRole('link', { name: letter.label, exact: true }).count(), 0);
  assert.equal((await api(learner, path, 'PATCH', {
    note: 'I tried a third round, too.', activity_date: null, expected_revision: moment.revision,
  })).status(), 200);
  await attach(parent, 'photo', 'new-photo.jpg');
  const amended = await api(guest, share).then((r) => r.json());
  assert.equal(amended.note, note);
  assert.deepEqual(amended.originals.map(({ id }) => id), [photo.id]);

  await parent.reload();
  await text(parent, 'Share with a relative').press('Enter');
  await button(parent, 'Open conversation').press('Enter');
  await visible(text(parent.getByRole('list', { name: 'Shared comments' }), reply));
  const replay = { body: 'A second reply.', submission_key: randomUUID() };
  const written = await api(guest, `${share}/comments`, 'POST', replay);
  assert.equal(written.status(), 201);
  const commentId = (await written.json()).id;
  assert.equal((await api(guest, `${share}/comments`, 'POST', replay).then((r) => r.json())).id, commentId);

  await button(parent, 'Withdraw share').press('Enter');
  await visible(parent.getByText('Fixture relative · Withdrawn', { exact: false }));
  const after = {
    photo: (await guest.request.get(photoPath)).status(),
    comment: (await api(guest, `${share}/comments`, 'POST', replay)).status(),
    visible_highlights: (await api(guest, '/shares').then((r) => r.json())).length,
  };
  assert.deepEqual(after, { photo: 404, comment: 404, visible_highlights: 0 });
  assert.equal((await api(guest, share)).status(), 404);
  assert.equal((await guest.request.head(photoPath)).status(), 404);
  assert.equal((await guest.request.get(photoPath, { headers: { Range: 'bytes=0-9' } })).status(), 404);
  assert.equal((await api(guest, `${share}/comments`)).status(), 404);
  assert.equal((await api(guest, `${share}/comments/${commentId}`, 'DELETE')).status(), 404);
  assert.equal((await api(guest, `${share}/comments/${commentId}/restore`, 'POST')).status(), 404);
  assert.equal((await api(guest, `${share}/accept`, 'POST', { token })).status(), 404);
  await guest.goto(`${origin}/shares/${id}/`);
  await visible(text(guest, 'This share is unavailable.'));
  await visible(text(guest, 'No shared items yet.'));
  assert.equal(await guest.getByRole('link', { name: photo.label }).count(), 0);
  assert.equal(hash(await parent.request.get(`/media/${photo.id}`).then((r) => r.body())), digest);
  await visible(text(parent.getByRole('main'), 'What would you try next?'));

  const expiredId = randomUUID();
  const invite = await api(parent, `/shares/${expiredId}`, 'PUT', {
    guest_id: actors.relative, moment_id: moment.id, revision: moment.revision + 1,
    include_text: false, original_asset_ids: [photo.id],
  });
  assert.equal(invite.status(), 201);
  const expired = await invite.json();
  sql(`UPDATE relative_shares SET invitation_expires_at = now() - interval '1 minute' WHERE id = '${expired.id}'`);
  for (let attempt = 0; attempt < 2; attempt++) {
    const denied = await api(guest, `/shares/${expired.id}/accept`, 'POST', { token: expired.token });
    assert.equal(denied.status(), 404);
    assert.equal((await denied.text()).includes(photo.label), false);
  }
  assert.equal((await guest.request.get(`/shares/${expired.id}/media/${photo.id}`)).status(), 404);
  assert.equal((await api(guest, '/shares').then((r) => r.json())).length, 0);
  await guest.getByLabel('Relative invitation code').fill(`${expired.id}.${expired.token}`);
  await button(guest, 'Accept invitation').press('Enter');
  await visible(text(guest, 'This invitation is unavailable.'));
  assert.equal(await guest.getByRole('link', { name: photo.label }).count(), 0);

  console.log(JSON.stringify(caseMode ? { before, after } : {
    before, after, fixture_ids: { highlight: moment.id, photo: photo.id, letter: letter.id, diary: diaryAsset, share: id, expired: expiredId },
  }));
} finally {
  await browser.close();
}
