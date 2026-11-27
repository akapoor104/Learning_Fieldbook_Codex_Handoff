import assert from 'node:assert/strict';
import { createServer, get } from 'node:http';
import { chromium } from 'playwright';

import { chromiumOptions, origin, provision, run, virtualAuthenticator } from './fixture.mjs';

const { actors, households } = provision();
const browser = await chromium.launch(chromiumOptions);
const prefix = '/api/v1';
const request = async (page, route, method = 'GET', body, csrf) =>
  page.evaluate(async ({ route, method, body, csrf }) => {
    const headers = {};
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (csrf) headers['X-CSRF-Token'] = csrf;
    const response = await fetch(route, { method, headers, body: JSON.stringify(body) });
    return {
      status: response.status,
      cache: response.headers.get('Cache-Control'),
      body: response.status === 204 ? null : await response.json(),
    };
  }, { route: prefix + route, method, body, csrf });

async function newActorBrowser() {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(origin + '/healthz');
  await virtualAuthenticator(page);
  return { context, page };
}

async function credential(page, options, enroll = false) {
  return page.evaluate(async ({ options, enroll }) => {
    const type = PublicKeyCredential;
    const result = enroll
      ? await navigator.credentials.create({ publicKey: type.parseCreationOptionsFromJSON(options) })
      : await navigator.credentials.get({ publicKey: type.parseRequestOptionsFromJSON(options) });
    return result.toJSON();
  }, { options, enroll });
}

async function enroll(page, token) {
  const start = await request(page, '/auth/enroll/start', 'POST', { token });
  assert.equal(start.status, 200);
  const { ceremony, options } = start.body;
  const result = { ceremony, credential: await credential(page, options.publicKey, true) };
  assert.equal((await request(page, '/auth/enroll/finish', 'POST', result)).status, 204);
  assert.equal((await request(page, '/auth/enroll/finish', 'POST', result)).status, 401);
  assert.equal((await request(page, '/auth/enroll/start', 'POST', { token })).status, 401);
}

async function signIn(page, username) {
  const start = await request(page, '/auth/sign-in/start', 'POST', { username });
  assert.equal(start.status, 200);
  const { ceremony, options } = start.body;
  const finish = { ceremony, credential: await credential(page, options.publicKey) };
  const result = await request(page, '/auth/sign-in/finish', 'POST', finish);
  assert.equal(result.status, 200);
  assert.equal(result.body.actor.id, actors[username]);
  assert.equal(result.body.actor.username, username);
  assert.equal(result.body.elevated_seconds, 0);
  assert.equal(result.cache, 'no-store, private');
  return { session: result.body, finish };
}

try {
  const wrongOrigin = await fetch(origin + prefix + '/auth/sign-in/start', {
    method: 'POST',
    headers: { Origin: 'https://unrelated.example', 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: 'learner' }),
  });
  assert.equal(wrongOrigin.status, 403);
  const wrongHost = await new Promise((resolve, reject) => {
    get(origin + prefix + '/session', { headers: { Host: 'unrelated.example' } }, (response) => {
      response.resume();
      resolve(response.statusCode);
    }).on('error', reject);
  });
  assert.equal(wrongHost, 403);
  const clients = new Map();
  for (const username of ['learner', 'parent-one', 'parent-two', 'other-learner', 'relative']) {
    const client = await newActorBrowser();
    const { context, page } = client;
    await enroll(page, run('account', 'enroll', actors[username]));
    const { session, finish } = await signIn(page, username);
    const [cookie] = await context.cookies();
    assert.deepEqual([cookie.name, cookie.httpOnly, cookie.secure, cookie.sameSite],
      ['__Host-fieldbook', true, true, 'Strict']);
    clients.set(username, { ...client, session, finish });
    console.log(`Verified independent passkey sign-in for ${username} (${session.actor.id})`);
  }
  const learner = clients.get('learner');
  const other = clients.get('other-learner');
  const guest = clients.get('relative');
  const own = await request(learner.page, `/households/${households.A}`);
  const inaccessible = await request(other.page, `/households/${households.A}`);
  const absent = await request(other.page, '/households/00000000-0000-0000-0000-000000000000');
  assert.equal(own.status, 200);
  assert.equal(inaccessible.status, 404);
  assert.deepEqual(inaccessible, absent);
  assert.deepEqual(await request(guest.page, `/households/${households.A}`), absent);
  assert.equal((await request(learner.page, '/auth/step-up/start', 'POST',
    undefined, learner.session.csrf_token)).status, 403);
  assert.equal((await request(learner.page, '/session', 'DELETE')).status, 403);
  assert.equal((await request(learner.page, '/auth/sign-in/start', 'POST',
    { username: 'parent-one' })).status, 409);
  console.log('Verified exact Origin and Host, CSRF, and indistinguishable private-record 404s', households);

  const parent = clients.get('parent-one');
  const otherParent = clients.get('parent-two');
  const [ordinary] = await parent.context.cookies();
  const start = await request(parent.page, '/auth/step-up/start', 'POST', undefined, parent.session.csrf_token);
  assert.equal(start.status, 200);
  const { ceremony, options } = start.body;
  const assertion = { ceremony, credential: await credential(parent.page, options.publicKey) };
  assert.equal((await request(otherParent.page, '/auth/step-up/finish', 'POST',
    assertion, otherParent.session.csrf_token)).status, 401);
  const elevated = await request(parent.page, '/auth/step-up/finish', 'POST',
    assertion, parent.session.csrf_token);
  assert.equal(elevated.status, 200);
  assert(elevated.body.elevated_seconds > 0 && elevated.body.elevated_seconds <= 300);
  assert.notEqual(elevated.body.csrf_token, parent.session.csrf_token);
  assert.equal((await request(parent.page, '/session', 'DELETE', undefined, parent.session.csrf_token)).status, 403);
  assert.equal((await parent.context.request.get(origin + prefix + '/session', {
    headers: { Cookie: `${ordinary.name}=${ordinary.value}` },
  })).status(), 401);
  assert.equal((await request(otherParent.page, '/session')).body.elevated_seconds, 0);

  assert.equal((await request(learner.page, '/session', 'DELETE',
    undefined, learner.session.csrf_token)).status, 204);
  assert.equal((await request(learner.page, '/auth/sign-in/finish', 'POST', learner.finish)).status, 401);

  // This localhost page shares an RP ID but has a different WebAuthn origin.
  const alternate = createServer((_, response) => response.end('Alternate test origin'));
  await new Promise((resolve) => alternate.listen(0, '127.0.0.1', resolve));
  try {
    const started = await request(learner.page, '/auth/sign-in/start', 'POST', { username: 'learner' });
    assert.equal(started.status, 200);
    await learner.page.goto(`http://localhost:${alternate.address().port}/`);
    const assertion = await credential(learner.page, started.body.options.publicKey);
    await learner.page.goto(origin + '/healthz');
    assert.equal((await request(learner.page, '/auth/sign-in/finish', 'POST', {
      ceremony: started.body.ceremony, credential: assertion,
    })).status, 401);
  } finally {
    await new Promise((resolve) => alternate.close(resolve));
  }
  const oldEnrollment = run('account', 'recover', actors.learner);
  const newEnrollment = run('account', 'recover', actors.learner);
  assert.equal((await request(learner.page, '/auth/enroll/start', 'POST', { token: oldEnrollment })).status, 401);
  assert.equal((await request(learner.page, '/auth/sign-in/start', 'POST', { username: 'learner' })).status, 401);
  const replacement = await newActorBrowser();
  await enroll(replacement.page, newEnrollment);
  await signIn(replacement.page, 'learner');
  run('account', 'revoke', actors.learner);
  assert.equal((await request(replacement.page, '/session')).status, 401);
  console.log('Verified WebAuthn origin binding, session rotation, logout, replay rejection, stable-ID recovery and revocation');
} finally {
  await browser.close();
}
