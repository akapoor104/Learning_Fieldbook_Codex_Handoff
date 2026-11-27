import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const origin = process.env.FIELDBOOK_TEST_ORIGIN;
export const database = process.env.FIELDBOOK_TEST_DATABASE_URL;
assert(origin && database, 'Set FIELDBOOK_TEST_ORIGIN and a disposable FIELDBOOK_TEST_DATABASE_URL');
for (const url of [origin, database]) {
  assert(['localhost', '127.0.0.1', '[::1]'].includes(new URL(url).hostname),
    'Browser tests require loopback services and a disposable database');
}
const root = fileURLToPath(new URL('../../', import.meta.url));
const binary = process.env.FIELDBOOK_TEST_BINARY ?? path.join(root, 'target/debug/fieldbook');
const fixture = JSON.parse(readFileSync(path.join(root, 'fixtures/household.json'), 'utf8'));
export const chromiumOptions = process.env.FIELDBOOK_TEST_CHROMIUM
  ? { executablePath: process.env.FIELDBOOK_TEST_CHROMIUM } : {};

export const runIn = (fixtureDatabase, ...args) => execFileSync(binary, args, {
  env: { ...process.env, DATABASE_URL: fixtureDatabase },
  encoding: 'utf8',
}).trim();
export const run = (...args) => runIn(database, ...args);

export function provision(prefix = '') {
  const households = Object.fromEntries(fixture.households.map(
    (alias) => [alias, run('account', 'household', `Fictional ${prefix}B1 household ${alias}`)],
  ));
  const actors = Object.fromEntries(fixture.actors.map(({ id, household, role }) => [
    id, run('account', 'add', households[household], prefix + id, `Fixture ${id}`, role),
  ]));
  return { households, actors };
}

export async function virtualAuthenticator(page, transport = 'usb') {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  const { authenticatorId } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2', transport, hasResidentKey: true,
      hasUserVerification: true, isUserVerified: true,
    },
  });
  return { cdp, authenticatorId };
}
