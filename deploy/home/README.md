# Run the home deployment skeleton

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

Use `docker compose down` to stop the stack and retain both volumes. This skeleton serves only status endpoints.
