<script lang="ts">
  import { flushSync, onMount, tick } from 'svelte';
  import { api, authenticate, authenticateForDiary, enroll, RequestError, supportsPasskeys,
    type DiaryAccessKind, type Session } from './identity';

  import Diary from './Diary.svelte';
  import Journal from './Journal.svelte';
  import ParentAccess from './ParentAccess.svelte';

  type View = 'home' | 'projects' | 'capture' | 'journal' | 'photos' | 'diary'
    | 'parent-access' | 'parent-diary' | 'parent-active';
  const parentAccessPath = '/diary/parent-access/';

  let session = $state<Session | null>(null);
  let checking = $state(true);
  let covered = $state(true);
  let busy = $state(false);
  let enrolling = $state(false);
  let warning = $state(false);
  let message = $state('');
  let error = $state('');
  let sessionCheckError = $state(false);
  let username = $state('');
  let invitation = $state('');
  let view = $state<View>('home');
  let entryDiary = false;
  let entryParentAccess = false;
  let requestedDiaryToken: string | null = null;
  let parentAccessEnded = $state(false);
  let entryMoment = $state<string | null>(null);
  let heading = $state<HTMLHeadingElement>();
  let nameField = $state<HTMLInputElement>();
  let inviteField = $state<HTMLInputElement>();
  let privateContent = $state<HTMLElement>();
  let revision = 0;
  let checkAfterAction = false;
  let notify: BroadcastChannel | undefined;
  let warningTimer: ReturnType<typeof setTimeout>;
  let expirationTimer: ReturnType<typeof setTimeout>;
  let elevationTimer: ReturnType<typeof setTimeout>;
  let diaryTimer: ReturnType<typeof setTimeout>;

  function hidePrivate() {
    covered = true;
    if (privateContent) privateContent.hidden = true;
  }

  function forget() {
    revision += 1;
    hidePrivate();
    clearTimeout(warningTimer);
    clearTimeout(expirationTimer);
    clearTimeout(elevationTimer);
    clearTimeout(diaryTimer);
    privateContent?.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input, textarea')
      .forEach((field) => field.value = '');
    session = null;
    sessionCheckError = false;
    username = '';
    invitation = '';
    enrolling = false;
    warning = false;
    view = 'home';
    entryDiary = false;
    entryParentAccess = false;
    requestedDiaryToken = null;
    parentAccessEnded = false;
    entryMoment = null;
  }

  function applySession(next: Session, requestedAt = performance.now()) {
    if (session && session.actor.id !== next.actor.id) forget();
    clearTimeout(warningTimer);
    clearTimeout(expirationTimer);
    clearTimeout(elevationTimer);
    clearTimeout(diaryTimer);
    session = next;
    let closingDiary = false;
    if (next.diary_learner_id) {
      view = window.location.pathname === parentAccessPath && requestedDiaryToken === next.csrf_token
        ? 'parent-diary' : 'parent-active';
      parentAccessEnded = false;
      entryMoment = null;

      // The server rounds up to a whole second. Budget transit time before revealing the view.
      const remaining = (next.diary_seconds - 1) * 1000 - (performance.now() - requestedAt);
      closingDiary = remaining <= 0;
      diaryTimer = setTimeout(() => recheck(true), closingDiary ? 250 : remaining);
    } else if (view === 'parent-diary' || view === 'parent-active') {
      view = 'home';
      requestedDiaryToken = null;
      parentAccessEnded = true;
    } else if (entryDiary && next.actor.role === 'learner') {
      view = 'diary';
    } else if (entryParentAccess && next.actor.role === 'guardian') {
      view = 'parent-access';
    } else if (entryMoment && next.actor.role !== 'guest') {
      view = 'journal';
    }
    entryDiary = false;
    entryParentAccess = false;
    sessionCheckError = false;
    covered = closingDiary || checkAfterAction || document.visibilityState === 'hidden';
    if (covered) hidePrivate();
    else if (privateContent?.hidden) {
      void tick().then(() => {
        if (!covered && privateContent && session?.csrf_token === next.csrf_token) privateContent.hidden = false;
      });
    }
    warning = next.idle_seconds <= 60;
    warningTimer = setTimeout(() => warning = true, Math.max(0, next.idle_seconds - 60) * 1000);
    expirationTimer = setTimeout(() => {
      forget();
      message = 'Your session ended. Sign in to continue.';
      void focusEntry();
    }, next.idle_seconds * 1000);
    if (next.elevated_seconds) {
      elevationTimer = setTimeout(() => {
        if (session?.csrf_token === next.csrf_token) session = { ...session, elevated_seconds: 0 };
      }, next.elevated_seconds * 1000);
    }
  }

  async function focusEntry() {
    await tick();
    if (enrolling) inviteField?.focus();
    else nameField?.focus();
  }

  async function verify(focus = false) {
    const started = ++revision;
    const requestedAt = performance.now();
    const hadSession = session !== null;
    hidePrivate();
    try {
      const current = await api<Session>('/session');
      if (started !== revision || document.visibilityState === 'hidden') return;
      applySession(current, requestedAt);
    } catch (failure) {
      if (started !== revision) return;
      if (failure instanceof RequestError && failure.status === 401) {
        forget();
        if (hadSession) {
          message = 'Your session ended. Sign in to continue.';
          focus = true;
        }
      } else if (hadSession) {
        sessionCheckError = true;
      } else {
        error = 'Fieldbook is unavailable. Reload to try again.';
      }
    } finally {
      checking = false;
    }
    if (focus) {
      await tick();
      if (session) heading?.focus();
      else await focusEntry();
    }
  }

  function readEntryLocation() {
    entryDiary = window.location.pathname === '/diary/';
    entryParentAccess = window.location.pathname === parentAccessPath;
    entryMoment = new URLSearchParams(window.location.search).get('moment');
  }

  onMount(() => {
    readEntryLocation();
    // Discard diary DOM and account state before Chromium can snapshot the page.
    function pageHide() {
      flushSync(() => {
        forget();
        checking = true;
      });
    }
    function pageShow(event: PageTransitionEvent) {
      if (event.persisted) {
        readEntryLocation();
        void verify(true);
      }
    }
    function visibilityChanged() {
      if (document.visibilityState === 'hidden') hidePrivate();
      else recheck();
    }
    if ('BroadcastChannel' in window) {
      notify = new BroadcastChannel('fieldbook-session');
      notify.onmessage = (event) => {
        if (event.data === 'session-ended') forget();
        recheck();
      };
    }
    window.addEventListener('pagehide', pageHide);
    window.addEventListener('pageshow', pageShow);
    document.addEventListener('visibilitychange', visibilityChanged);
    void verify(true);
    return () => {
      forget();
      notify?.close();
      window.removeEventListener('pagehide', pageHide);
      window.removeEventListener('pageshow', pageShow);
      document.removeEventListener('visibilitychange', visibilityChanged);
    };
  });

  function recheck(focus = false) {
    if (busy) {
      hidePrivate();
      checkAfterAction = true;
    } else {
      void verify(focus);
    }
  }

  function finishAction() {
    busy = false;
    if (checkAfterAction) {
      checkAfterAction = false;
      if (document.visibilityState !== 'hidden') void verify();
    }
  }

  async function submitEntry(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    if (!supportsPasskeys()) {
      error = 'Open Fieldbook in a browser that supports passkeys.';
      return;
    }
    error = '';
    message = '';
    busy = true;
    const started = ++revision;
    try {
      if (enrolling) {
        await enroll(invitation.trim());
        if (started !== revision) return;
        enrolling = false;
        invitation = '';
        message = 'Passkey added. Sign in with your account name.';
        await focusEntry();
      } else {
        const next = await authenticate(username.trim().toLowerCase());
        if (started !== revision) return;
        if (['/diary/', parentAccessPath].includes(window.location.pathname)) readEntryLocation();
        applySession(next);
        username = '';
        await tick();
        heading?.focus();
        notify?.postMessage('session-changed');
      }
    } catch (failure) {
      if (started !== revision) return;
      if (failure instanceof RequestError && failure.status === 409 && !enrolling) {
        await verify(true);
      } else {
        error = enrolling
          ? 'Passkey setup could not finish. Check the setup code or ask for a new one.'
          : 'Sign-in could not finish. Check your account name and try your passkey again.';
      }
    } finally {
      finishAction();
    }
  }

  async function leave(switching: boolean) {
    if (!session || busy) return;
    const csrf = session.csrf_token;
    busy = true;
    error = '';
    message = '';
    forget();
    try {
      await api<void>('/session', 'DELETE', undefined, csrf);
      message = switching ? 'Sign in with a different account.' : 'You signed out.';
      notify?.postMessage('session-ended');
      await focusEntry();
    } catch {
      await verify(true);
      error = 'Sign-out could not be confirmed. Try again.';
    } finally {
      finishAction();
    }
  }

  async function refreshSession(reauthenticate: boolean) {
    if (!session || busy) return;
    const csrf = session.csrf_token;
    busy = true;
    error = '';
    const started = ++revision;
    const requestedAt = performance.now();
    try {
      const next = reauthenticate
        ? await authenticate('', csrf)
        : await api<Session>('/session/keep-alive', 'POST', undefined, csrf);
      if (started !== revision) return;
      applySession(next, requestedAt);
      if (reauthenticate) notify?.postMessage('session-changed');
    } catch (failure) {
      if (started !== revision) return;
      if (failure instanceof RequestError && (failure.status === 401 || failure.status === 403)) {
        await verify(true);
      } else {
        error = 'Your session could not be verified. Try again.';
        if (covered) await verify();
      }
    } finally {
      finishAction();
    }
  }

  async function openParentDiary(learner: string, kind: DiaryAccessKind) {
    if (!session || busy || covered || view !== 'parent-access') return;
    if (!supportsPasskeys()) {
      error = 'Open Fieldbook in a browser that supports passkeys.';
      return;
    }
    const csrf = session.csrf_token;
    busy = true;
    error = '';
    const started = ++revision;
    try {
      const { session: next, requestedAt } = await authenticateForDiary(learner, kind, csrf);
      if (started !== revision) return;
      requestedDiaryToken = next.csrf_token;
      applySession(next, requestedAt);
      notify?.postMessage('session-changed');
    } catch (failure) {
      if (started !== revision) return;
      if (failure instanceof RequestError && [401, 403].includes(failure.status)) await verify(true);
      error = 'Parent access could not open. Try again with your passkey.';
    } finally {
      finishAction();
    }
  }

  function openActiveDiary() {
    if (!session?.diary_learner_id || covered || busy) return;
    requestedDiaryToken = session.csrf_token;
    recheck();
  }

  async function open(next: View) {
    if (!session || covered || session.diary_learner_id) return;
    parentAccessEnded = false;
    view = next;
    await tick();
    if (next === 'home' || next === 'projects') heading?.focus();
  }

  async function changeEntry(next: boolean) {
    enrolling = next;
    invitation = '';
    message = '';
    error = '';
    await focusEntry();
  }
</script>

<div class="shell">
  <header class="brand">Learning Fieldbook</header>
  {#if checking}
    <main><p role="status">Checking your session…</p></main>
  {:else if session}
    {#if covered && sessionCheckError}
      <main>
        <p role="alert">Fieldbook could not check your session.</p>
        <button type="button" onclick={() => verify(true)}>Try again</button>
      </main>
    {:else if covered && session.diary_learner_id}
      <main><p role="status">Checking diary access…</p></main>
    {/if}
    <div class="notebook" bind:this={privateContent} hidden={covered}>
      <header class="account">
        <p>Signed in as <strong>{session.actor.display_name}</strong></p>
        <div class="actions">
          <button type="button" class="quiet" onclick={() => leave(true)} disabled={busy}>Switch account</button>
          <button type="button" class="quiet" onclick={() => leave(false)} disabled={busy}>Sign out</button>
        </div>
      </header>
      {#if session.actor.role !== 'guest' && !session.diary_learner_id}
        <nav aria-label="Notebook">
          <button type="button" class="quiet" aria-current={view === 'home' ? 'page' : undefined} onclick={() => open('home')}>Home</button>
          <button type="button" class="quiet" aria-current={view === 'projects' ? 'page' : undefined} onclick={() => open('projects')}>Projects</button>
        </nav>
      {/if}
      <main>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        {#if warning && !session.diary_learner_id}
          <aside class="notice">
            <p role="status">Your session will end soon.</p>
            <button type="button" class="quiet" disabled={busy} onclick={() => refreshSession(false)}>Stay signed in</button>
          </aside>
        {/if}
        {#if session.diary_learner_id}
          {#if !covered && view === 'parent-diary'}
            {#key session.csrf_token}
              <Diary {session} onauthlost={() => recheck(true)} />
            {/key}
          {:else if view === 'parent-active'}
            <h1 bind:this={heading} tabindex="-1">Parent access is active</h1>
            {#if window.location.pathname === parentAccessPath}
              <button type="button" onclick={openActiveDiary}>Open the learner’s diary</button>
            {:else}
              <a href={parentAccessPath}>Go to Parent access</a>
            {/if}
          {/if}
        {:else if view === 'projects'}
          <h1 bind:this={heading} tabindex="-1">Projects</h1>
          <p>No projects yet.</p>
        {:else if view === 'home'}
          <h1 bind:this={heading} tabindex="-1">Home</h1>
          {#if parentAccessEnded}<p role="status">Parent access ended.</p>{/if}
          {#if session.actor.role === 'guest'}
            <p>No shared items yet.</p>
          {:else}
            <h2>{session.actor.role === 'learner' ? 'What would you like to remember?' : 'Working together'}</h2>
            <div class="actions">
              <button type="button" onclick={() => open('capture')}>Write a sentence</button>
              <button type="button" class="quiet" onclick={() => open('journal')}>Family journal</button>
              <button type="button" class="quiet" onclick={() => open('photos')}>Family photos to match</button>
              {#if session.actor.role === 'learner'}
                <button type="button" class="quiet" onclick={() => open('diary')}>My diary</button>
              {:else}
                <a class="parent-link" href={parentAccessPath}>Parent access</a>
              {/if}
            </div>
            {#if session.actor.role === 'guardian'}
              <div class="reauth">
                <button type="button" class="quiet" disabled={busy} onclick={() => refreshSession(true)}>Verify it’s you</button>
                {#if session.elevated_seconds > 0}<p role="status">Identity verified.</p>{/if}
              </div>
            {/if}
          {/if}
        {:else if !covered && view === 'diary' && session.actor.role === 'learner'}
          {#key session.csrf_token}
            <Diary {session} onauthlost={() => recheck(true)} />
          {/key}
        {:else if !covered && view === 'parent-access' && session.actor.role === 'guardian'}
          <ParentAccess {session} {busy} onopen={openParentDiary} onauthlost={() => recheck(true)} />
        {/if}
        {#if session.actor.role !== 'guest' && !session.diary_learner_id}
          {#key session.actor.id}
            <Journal {session} panel={view === 'capture' || view === 'journal' || view === 'photos' ? view : null}
              {entryMoment} onvisited={() => entryMoment = null}
              onjournal={() => open('journal')} onauthlost={() => verify(true)} />
          {/key}
        {/if}
      </main>
    </div>
  {:else}
    <main class="entry">
      <h1>{enrolling ? 'Set up your passkey' : 'Sign in'}</h1>
      {#if message}<p role="status">{message}</p>{/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <form onsubmit={submitEntry}>
        {#if enrolling}
          <label for="invitation">Passkey setup code</label>
          <input id="invitation" type="password" bind:this={inviteField} bind:value={invitation} required autocomplete="off" />
        {:else}
          <label for="username">Account name</label>
          <input id="username" bind:this={nameField} bind:value={username} required autocomplete="username" autocapitalize="none" spellcheck="false" />
          <p class="hint">Use this device’s passkey or your security key.</p>
        {/if}
        <button type="submit" disabled={busy}>{enrolling ? 'Add passkey' : 'Sign in with a passkey'}</button>
      </form>
      <button type="button" class="quiet entry-option" disabled={busy} onclick={() => changeEntry(!enrolling)}>
        {enrolling ? 'Back to sign in' : 'I have a passkey setup code'}
      </button>
    </main>
  {/if}
</div>

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) { margin: 0; color: #252923; background: #f4f3eb; font: 1.0625rem/1.55 system-ui, sans-serif; }
  :global(:focus-visible) { outline: 3px solid #9e481d; outline-offset: 3px; }
  [hidden] { display: none; }
  .shell { max-width: 58rem; margin: auto; padding: clamp(.75rem, 4vw, 2.5rem); }
  .brand { color: #20492d; font-weight: 750; font-size: 1.15rem; margin-bottom: 1.25rem; }
  main { border: 1px solid #b8bbae; background: #fffcf5; border-radius: 1rem; padding: clamp(1rem, 4vw, 2rem); }
  .entry { max-width: 31rem; }
  h1 { font: 700 clamp(1.75rem, 5vw, 2.5rem)/1.15 Georgia, serif; margin: 0 0 1rem; }
  h2 { font-size: 1.25rem; margin-top: 0; }
  p { margin: .5rem 0 1rem; }
  label { display: block; font-weight: 650; margin: .5rem 0; }
  :global(.shell input), :global(.shell textarea), :global(.shell select) { display: block; width: 100%; font: inherit; min-height: 44px; border: 1px solid #646d60; border-radius: .4rem; padding: .6rem; color: inherit; background: white; }
  :global(.shell textarea) { resize: vertical; max-width: 44rem; }
  :global(.shell button) { font: inherit; font-weight: 650; min-height: 44px; min-width: 44px; border: 1px solid #20492d; border-radius: .45rem; color: white; background: #20492d; padding: .45rem .75rem; cursor: pointer; }
  :global(.shell button:hover) { background: #153620; }
  :global(.shell button:disabled) { cursor: wait; }
  :global(.shell button.quiet) { color: #20492d; background: #f4f3eb; }
  :global(.shell button.quiet:hover), button[aria-current] { background: #dfe8d9; }
  .parent-link { display: inline-flex; align-items: center; min-height: 44px; color: #20492d; padding-inline: .5rem; }
  form > button, .entry-option, .reauth { margin-top: 1rem; }
  .hint { color: #483f34; font-size: 1rem; }
  .account { display: flex; align-items: center; justify-content: space-between; gap: .6rem; flex-wrap: wrap; }
  .account p { margin: 0; overflow-wrap: anywhere; }
  .actions, nav { display: flex; flex-wrap: wrap; gap: .5rem; }
  nav, .account { margin-bottom: 1rem; }
  .notice { background: #f7ead6; border: 1px solid #9e481d; padding: .75rem; margin-bottom: 1rem; }
  .notice p { margin: 0 0 .5rem; }
  .error { color: #8b2620; }
</style>
