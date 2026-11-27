# Run Fieldbook at home

Run these commands from the repository root with Docker Engine and Compose v2. Use fictional data for this rehearsal. The web port is bound to localhost. PostgreSQL and the private media directory have separate named volumes.

On Windows with Docker Desktop running, WSL can use `docker.exe compose` for these commands if the Linux `docker` command is unavailable. Open the published localhost URL from Windows.

Create a local `.env` once for a new installation. Skip this command if you already have one. The hex password works directly in the PostgreSQL URL.

```sh
(umask 077; set -C; printf 'FIELDBOOK_DB_PASSWORD=%s\n' "$(openssl rand -hex 24)" > .env)
```

Validate the Compose definition, start PostgreSQL, build the image, and apply the migration before starting the web service.

```sh
docker compose --env-file .env.example config --quiet
docker compose up -d --wait db
docker compose build web
docker compose run --rm web fieldbook migrate
docker compose up -d --wait web
python3 tools/probe_health.py http://127.0.0.1:8080
```

`fieldbook migrate` can be rerun. `/healthz` checks the process. `/readyz` checks the database and applied schema. Use the port in `FIELDBOOK_HTTP_PORT` if you change it.

To rehearse a database outage, stop the database while leaving the web service running, probe both endpoints, then start it again.

```sh
docker compose stop db
python3 tools/probe_health.py http://127.0.0.1:8080 --expect-not-ready
docker compose start db
```

Use `docker compose down` to stop the stack and retain both volumes.

## Enable the signed-in notebook

For a fictional rehearsal on the host computer, set these variables before starting the service. Use your chosen published port if it differs from 8080. For the private home deployment, use its canonical HTTPS origin and matching hostname as the RP ID. Keep the session secret stable across restarts and preserve the public Host header at the reverse proxy.

```sh
export FIELD_PUBLIC_ORIGIN=http://localhost:8080
export FIELD_RP_ID=localhost
export FIELD_SESSION_SECRET="$(openssl rand -hex 32)"
docker compose up -d --wait web
```

Run the account commands from the trusted operator terminal. Each household and actor creation command prints its stable ID. Each person enters their own short-lived setup code under **I have a passkey setup code** and verifies with their own device or security key.

```sh
household=$(docker compose exec -T web fieldbook account household 'Fictional family')
learner=$(docker compose exec -T web fieldbook account add "$household" learner 'Fictional learner' learner)
parent_one=$(docker compose exec -T web fieldbook account add "$household" parent-one 'Fictional guardian one' guardian)
parent_two=$(docker compose exec -T web fieldbook account add "$household" parent-two 'Fictional guardian two' guardian)
docker compose exec -T web fieldbook account enroll "$learner"
docker compose exec -T web fieldbook account enroll "$parent_one"
docker compose exec -T web fieldbook account enroll "$parent_two"
```

If the learner loses a credential, use `fieldbook account recover "$learner"` in the same operator context. It ends every session for the account and issues a fresh setup code for the same actor ID. `fieldbook account revoke "$learner"` ends those sessions and disables the credential until a new setup code is issued. Keep setup codes out of logs and receipts. Open the configured origin in the browser after migrating.

## Run a published release

For a rehearsal that compares hosts, set `FIELD_IMAGE` to the complete Artifact Registry reference pinned by digest used in Render and Google Cloud. Authenticate Docker to that registry first. The build command above is for local development.

```sh
docker compose pull web
docker compose up -d --wait db
docker compose run --rm --pull never web fieldbook migrate
docker compose up -d --wait --no-build web
python3 tools/probe_health.py http://127.0.0.1:8080
```

The release expects its named media volume at `/var/data/fieldbook`. Starting the image with filesystem storage and only its container filesystem, or a tmpfs at that mount point, must exit before serving.
