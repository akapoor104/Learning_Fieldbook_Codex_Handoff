export interface Session {
  actor: {
    id: string;
    household_id: string;
    username: string;
    display_name: string;
    role: 'learner' | 'guardian' | 'guest';
  };
  csrf_token: string;
  idle_seconds: number;
  elevated_seconds: number;
}

export class RequestError extends Error {
  constructor(readonly status: number) {
    super(`Fieldbook request returned ${status}`);
  }
}

export async function api<T>(route: string, method = 'GET', body?: object, csrf?: string): Promise<T> {
  const headers = new Headers({ Accept: 'application/json' });
  if (body) headers.set('Content-Type', 'application/json');
  if (csrf) headers.set('X-CSRF-Token', csrf);
  const response = await fetch(`/api/v1${route}`, {
    method,
    headers,
    body: JSON.stringify(body),
    cache: 'no-store',
    credentials: 'same-origin',
  });
  if (!response.ok) throw new RequestError(response.status);
  return response.status === 204 ? undefined as T : response.json();
}

export function supportsPasskeys() {
  return !!globalThis.PublicKeyCredential?.parseCreationOptionsFromJSON
    && !!globalThis.PublicKeyCredential?.parseRequestOptionsFromJSON;
}

interface Ceremony<T> {
  ceremony: string;
  options: { publicKey: T };
}

export async function enroll(token: string) {
  const { ceremony, options } = await api<Ceremony<PublicKeyCredentialCreationOptionsJSON>>(
    '/auth/enroll/start', 'POST', { token },
  );
  const publicKey = PublicKeyCredential.parseCreationOptionsFromJSON(options.publicKey);
  const credential = await navigator.credentials.create({ publicKey });
  if (!(credential instanceof PublicKeyCredential)) throw new Error('No passkey returned');
  return api<void>('/auth/enroll/finish', 'POST', { ceremony, credential: credential.toJSON() });
}

export async function authenticate(username: string, csrf?: string) {
  const route = csrf ? '/auth/step-up' : '/auth/sign-in';
  const { ceremony, options } = await api<Ceremony<PublicKeyCredentialRequestOptionsJSON>>(
    `${route}/start`, 'POST', csrf ? undefined : { username }, csrf,
  );
  const publicKey = PublicKeyCredential.parseRequestOptionsFromJSON(options.publicKey);
  const credential = await navigator.credentials.get({ publicKey });
  if (!(credential instanceof PublicKeyCredential)) throw new Error('No passkey returned');
  return api<Session>(`${route}/finish`, 'POST', { ceremony, credential: credential.toJSON() }, csrf);
}
