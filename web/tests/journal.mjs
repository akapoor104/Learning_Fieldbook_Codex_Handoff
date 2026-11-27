import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { chromiumOptions, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const caseMode = process.argv.includes('--case');
if (caseMode) {
  const packet = JSON.parse(readFileSync(0, 'utf8'));
  assert.equal(packet.case, 'journal-retry');
  assert.equal(packet.input.fixture, 'household');
  assert.equal(packet.input.actor, 'learner');
}
const accountPrefix = caseMode ? 'retry-' : 'journal-';
const { households, actors } = provision(accountPrefix);
const browser = await chromium.launch(chromiumOptions);
const button = (page, name) => page.getByRole('button', { name, exact: true });
const sentence = (page) => page.getByRole('textbox', { name: 'What happened?' });
const visible = (locator) => locator.waitFor({ state: 'visible', timeout: 10000 });

async function signIn(alias) {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(origin);
  await virtualAuthenticator(page);
  await button(page, 'I have a passkey setup code').click();
  await page.getByLabel('Passkey setup code').fill(run('account', 'enroll', actors[alias]));
  await button(page, 'Add passkey').click();
  await visible(page.getByText('Passkey added. Sign in with your account name.'));
  const account = page.getByLabel('Account name');
  await account.fill(accountPrefix + alias);
  await account.press('Tab');
  await page.keyboard.press('Enter');
  await visible(page.getByRole('heading', { name: 'Home', exact: true }));
  return { context, page };
}

async function getMoments(page) {
  return page.evaluate(async () => {
    const response = await fetch('/api/v1/moments', { cache: 'no-store' });
    return response.json();
  });
}

async function request(page, path, method = 'GET', body) {
  return page.evaluate(async ({ path, method, body }) => {
    const session = await fetch('/api/v1/session').then((response) => response.json());
    const response = await fetch(`/api/v1${path}`, {
      method,
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': session.csrf_token },
      body: JSON.stringify(body),
    });
    return { status: response.status, body: response.status === 204 ? null : await response.json() };
  }, { path, method, body });
}

async function openMoment(page, wording) {
  await button(page, 'Home').click();
  await button(page, 'Family journal').click();
  const back = button(page, 'Back to family journal');
  if (await back.count()) await back.click();
  await page.getByRole('button').filter({ hasText: wording }).click();
  await visible(page.getByRole('heading', { name: 'Moment', exact: true }));
}

async function edit(page, words) {
  await button(page, 'Edit words').click();
  await page.getByLabel('Your words').fill(words);
}

async function journalRetryCase() {
  const { page } = await signIn('learner');
  const before = await getMoments(page);
  const command = {
    submission_key: randomUUID(),
    note: 'I lined up the left edges so the pieces were easier to compare.',
  };
  const [first, second] = await Promise.all([
    request(page, '/moments', 'POST', command),
    request(page, '/moments', 'POST', command),
  ]);
  assert.equal(first.status, 201);
  assert.equal(second.status, 201);
  const reloaded = await request(page, `/moments/${first.body.id}`);
  assert.equal(reloaded.status, 200);
  assert.equal(reloaded.body.recorder_id, actors.learner);
  assert.equal(reloaded.body.note, command.note);
  const after = await getMoments(page);
  console.log(JSON.stringify({
    new_moments: after.length - before.length,
    same_id: first.body.id === second.body.id && first.body.id === reloaded.body.id,
    voice: reloaded.body.voice,
    added_assets: reloaded.body.asset_ids.length,
  }));
}

async function journalBrowserSuite() {
  const secondLearner = run('account', 'add', households.A, 'journal-second-learner', 'Second fictional learner', 'learner');
  const { context: learnerContext, page: learner } = await signIn('learner');
  await learner.keyboard.press('Tab');
  assert(await button(learner, 'Write a sentence').evaluate((element) => element === document.activeElement));
  await learner.keyboard.press('Enter');
  await visible(sentence(learner));
  assert(await sentence(learner).evaluate((element) => element === document.activeElement));
  const tooLong = 'x'.repeat(16001);
  await sentence(learner).fill(tooLong);
  await button(learner, 'Save moment').click();
  await visible(learner.getByRole('alert'));
  assert.equal(await sentence(learner).inputValue(), tooLong);

  const note = 'I lined up the left edges so the pieces were easier to compare.';
  await sentence(learner).fill(note);
  await button(learner, 'Save moment').dblclick();
  await visible(learner.getByText('Moment saved.', { exact: true }));
  await visible(learner.getByText(note, { exact: true }));
  const [moment] = await getMoments(learner);
  assert.equal((await getMoments(learner)).length, 1);
  assert.equal(moment.note, note);
  assert.equal(moment.voice, 'student');
  assert.equal(moment.recorder_id, actors.learner);
  assert.deepEqual(moment.asset_ids, []);
  assert.equal(moment.activity_date, null);

  const { context: guardianContext, page: guardian } = await signIn('parent-one');
  await button(guardian, 'Write a sentence').click();
  await guardian.getByLabel('Learner', { exact: true }).selectOption(actors.learner);
  await sentence(guardian).fill('First learner’s in-progress observation.');
  await guardian.getByLabel('Activity date (optional)').fill('2026-01-10');

  const otherTab = await guardianContext.newPage();
  await otherTab.goto(origin);
  await button(otherTab, 'Write a sentence').click();
  await otherTab.getByLabel('Learner', { exact: true }).selectOption(secondLearner);
  await sentence(otherTab).fill('A separate tab and learner.');
  assert.equal(await sentence(otherTab).inputValue(), 'A separate tab and learner.');
  await guardian.bringToFront();
  assert.equal(await sentence(guardian).inputValue(), 'First learner’s in-progress observation.');
  await guardian.getByLabel('Learner', { exact: true }).selectOption(secondLearner);
  assert.equal(await sentence(guardian).inputValue(), '');
  await guardian.getByLabel('Learner', { exact: true }).selectOption(actors.learner);
  await button(guardian, 'Projects').click();
  await button(guardian, 'Home').click();
  await button(guardian, 'Write a sentence').click();
  assert.equal(await sentence(guardian).inputValue(), 'First learner’s in-progress observation.');
  assert.equal(await guardian.getByLabel('Activity date (optional)').inputValue(), '2026-01-10');
  await guardian.route('**/api/v1/session', (route) => route.fulfill({ status: 503, json: { error: 'unavailable' } }));
  await guardian.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await visible(guardian.getByText('Fieldbook could not check your session.'));
  assert.equal(await sentence(guardian).count(), 0);
  await guardian.unroute('**/api/v1/session');
  await button(guardian, 'Try again').click();
  assert.equal(await sentence(guardian).inputValue(), 'First learner’s in-progress observation.');
  await button(guardian, 'Save moment').click();
  await visible(guardian.getByText('Moment saved.', { exact: true }));
  const entries = await getMoments(guardian);
  const observation = entries.find(({ recorder_id }) => recorder_id === actors['parent-one']);
  assert.equal(observation.note, 'First learner’s in-progress observation.');
  assert.equal(observation.voice, 'guardian');
  assert.equal(observation.learner_id, actors.learner);
  assert.equal(observation.activity_date, '2026-01-10');

  await openMoment(guardian, note);
  assert.equal(await button(guardian, 'Edit words').count(), 0);
  const denied = await request(guardian, `/moments/${moment.id}`, 'PATCH', {
    expected_revision: 1, note: 'Guardian replacement.',
  });
  assert.equal(denied.status, 404);
  const suggestion = 'Could you try lining the pieces up from the other end?';
  await guardian.getByLabel('Reply type').selectOption('suggestion');
  await guardian.getByLabel('Your comment').fill(suggestion);
  await button(guardian, 'Post comment').click();
  await visible(guardian.getByText(suggestion, { exact: true }));
  const commentsPath = `/moments/${moment.id}/comments`;
  const [posted] = (await request(learner, commentsPath)).body;
  assert.equal(posted.author_id, actors['parent-one']);
  assert.equal(posted.kind, 'suggestion');
  assert.equal(posted.body, suggestion);
  assert.equal((await request(learner, `/moments/${moment.id}`)).body.note, note);
  await button(guardian, 'Withdraw comment').click();
  await visible(guardian.getByText('Comment withdrawn.'));
  assert.deepEqual((await request(learner, commentsPath)).body, []);
  assert.equal((await request(learner, `${commentsPath}/${posted.id}`)).status, 404);
  await button(guardian, 'Restore comment').click();
  await visible(guardian.getByText('Comment restored.'));
  assert.equal((await request(learner, commentsPath)).body[0].id, posted.id);

  await edit(learner, 'My revision from the earlier tab.');
  const editingTab = await learnerContext.newPage();
  await editingTab.goto(origin);
  await openMoment(editingTab, note);
  const newerWords = 'I checked both sides after moving the pieces.';
  await edit(editingTab, newerWords);
  await button(editingTab, 'Save changes').click();
  await visible(editingTab.getByText('Changes saved.'));
  await learner.bringToFront();
  await button(learner, 'Save changes').click();
  await visible(learner.getByText('Saved version 2', { exact: true }));
  assert.equal(await learner.getByLabel('Your words').inputValue(), 'My revision from the earlier tab.');
  await button(learner, 'Keep my draft and use the latest revision').click();
  await button(learner, 'Save changes').click();
  await visible(learner.getByText('Changes saved.'));
  await button(learner, 'Earlier versions').click();
  await visible(learner.getByText(note, { exact: true }));
  await button(learner, 'Restore version 1').click();
  await button(learner, 'Save changes').click();
  await visible(learner.getByText('Changes saved.'));
  const restored = (await request(learner, `/moments/${moment.id}`)).body;
  assert.equal(restored.revision, 4);
  assert.equal(restored.note, note);
  assert.equal(restored.recorder_id, actors.learner);

  const spoken = 'I counted the gaps before changing my plan.';
  await button(guardian, 'Home').click();
  await button(guardian, 'Write a sentence').click();
  await guardian.getByLabel('Learner', { exact: true }).selectOption(actors.learner);
  await sentence(guardian).fill(spoken);
  await guardian.getByText('These are the learner’s spoken words').click();
  await button(guardian, 'Save moment').click();
  await visible(guardian.getByText('Awaiting learner confirmation.'));
  const scribed = (await getMoments(guardian)).find((item) => item.note === spoken);
  assert.equal(scribed.voice, 'scribed');
  const confirmation = `/moments/${scribed.id}/confirmation`;
  assert.equal((await request(guardian, confirmation, 'POST', { expected_revision: 1 })).status, 404);
  await openMoment(learner, spoken);
  await button(learner, 'Confirm these words').click();
  await visible(learner.getByText('Words confirmed.'));
  assert.equal((await request(learner, `/moments/${scribed.id}`)).body.confirmed, true);
  await edit(learner, 'I counted the gaps twice before changing my plan.');
  await button(learner, 'Save changes').click();
  await visible(learner.getByText('Awaiting learner confirmation.'));
  const staleConfirmation = await request(learner, confirmation, 'POST', { expected_revision: 1 });
  assert.equal(staleConfirmation.status, 409);
  assert.equal(staleConfirmation.body.saved.revision, 2);
  const revisions = (await request(learner, `/moments/${scribed.id}/revisions`)).body;
  assert.deepEqual(revisions.map(({ confirmed }) => confirmed), [false, false]);
  assert.equal(revisions[0].editor_id, actors.learner);
  assert.equal(revisions[0].recorder_id, actors['parent-one']);
  assert.equal(revisions[1].note, spoken);
  await button(learner, 'Confirm these words').click();
  await visible(learner.getByText('Words confirmed.'));
  await button(learner, 'Withdraw confirmation').click();
  await visible(learner.getByText('Confirmation withdrawn.'));

  const { page: outsider } = await signIn('other-learner');
  const foreign = await request(outsider, `/moments/${moment.id}`);
  const absent = await request(outsider, '/moments/00000000-0000-0000-0000-000000000000');
  assert.equal(foreign.status, 404);
  assert.deepEqual(foreign, absent);
  assert.deepEqual(await request(outsider, commentsPath), absent);
  assert.deepEqual(await request(outsider, `/moments/${moment.id}/revisions`), absent);
  console.log('Signed suggestion, author-only revision and recovery, scribed confirmation, and scoped journal 404 passed');

  const formNote = 'A moment sent from the browser form entry.';
  const redirect = await learner.evaluate(async (note) => {
    const session = await fetch('/api/v1/session').then((response) => response.json());
    const fields = {
      csrf_token: session.csrf_token, submission_key: crypto.randomUUID(), note,
    };
    const invalid = await fetch('/moments/new/', {
      method: 'POST', body: new URLSearchParams({ ...fields, activity_date: '2026-02-30' }),
    });
    const rejected = { status: invalid.status, body: await invalid.json() };
    const valid = await fetch('/moments/new/', {
      method: 'POST', body: new URLSearchParams(fields),
    });
    return { rejected, url: valid.url, redirected: valid.redirected };
  }, formNote);
  assert.equal(redirect.rejected.status, 422);
  assert.equal(redirect.rejected.body.note, formNote);
  assert.equal(redirect.redirected, true);
  assert.match(redirect.url, /[?]moment=[0-9a-f-]{36}$/);
  await learner.goto(redirect.url);
  await visible(learner.getByRole('heading', { name: 'Moment', exact: true }));
  await visible(learner.getByText(formNote, { exact: true }));


  await button(guardian, 'Switch account').click();
  await guardian.getByLabel('Account name').waitFor();
  await otherTab.bringToFront();
  await otherTab.getByLabel('Account name').waitFor();
  assert.equal(await sentence(otherTab).count(), 0);
  assert.deepEqual(await otherTab.evaluate(() => [localStorage.length, sessionStorage.length]), [0, 0]);
  console.log('Keyboard capture, double-tap, 422 draft retention, two-tab learner selection and account switch passed');
  console.log('Journal fixture', { household: households.A, learner: actors.learner, moment: moment.id, observation: observation.id, scribed: scribed.id, comment: posted.id });
}

try {
  await (caseMode ? journalRetryCase() : journalBrowserSuite());
} finally {
  await browser.close();
}
