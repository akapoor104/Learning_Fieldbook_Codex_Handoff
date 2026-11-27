# Google Cloud Run

Use a billing-enabled project dedicated to fictional staging data for the first rehearsal. Use separate Terraform state for staging and production. Terraform manages the Cloud SQL instance and database, private media bucket, runtime identity, secret containers and grants, migration job, and web service. The runtime uses the authenticated Cloud SQL connector over an instance socket. The SQL instance has a public IP for connector reachability and an empty authorized network list. Cloud Run starts at zero instances and is capped at two.

## Publish the shared release

Build the AMD64 image from the repository root and push it once to an existing Artifact Registry Docker repository. Authenticate Docker to the registry first. Give each host the complete `…/fieldbook@sha256:…` reference reported by the inspection command, including the registry and repository. Use the same reference in the two Google containers.

```sh
docker buildx build --platform linux/amd64 --provenance=false --push -t "$release_tag" .
docker buildx imagetools inspect "$release_tag"
```

For a repository in a separate project, give the deployer and workload project's Cloud Run service agent Artifact Registry Reader on that repository. Enable the Run API and create the workload project's service identity first, using `gcloud beta services identity create --service=run.googleapis.com --project="$project"`. The agent is `service-PROJECT_NUMBER@serverless-robot-prod.iam.gserviceaccount.com`. Give Render and the home operator their own read credentials.

## Validate and provision

With Terraform and the gcloud CLI installed, run from `deploy/google`.

```sh
terraform fmt -check
terraform init -backend=false
terraform validate
```

For an authorized rehearsal, initialize Terraform with the selected state's backend, authenticate Terraform with Application Default Credentials, and authenticate gcloud for the same project. Copy the matching `.tfvars.example` to `staging.tfvars` or `production.tfvars`. Replace the sample project, image, bucket, and origin. Use a unique bucket name. For staging, the default deterministic Cloud Run URL is `https://fieldbook-web-PROJECT_NUMBER.us-west1.run.app` when available. Verify the assigned URL from the service output before using that origin for sign-in. Keep the origin and RP ID stable for a real installation.

Set `vars=staging.tfvars`, `project` to its project ID and `region` to its region. Review the full plan, then bootstrap the database and secret containers.

```sh
terraform plan -var-file="$vars"
terraform apply -var-file="$vars" \
  -target=google_sql_database.fieldbook \
  -target=google_secret_manager_secret.runtime
```

Set a temporary administrator credential through Cloud SQL and connect as `postgres` via the Cloud SQL Auth Proxy. In the `fieldbook` database create the application login and grant it the database and schema permissions needed for migrations. Set its password with psql's interactive `\password fieldbook` command.

```sql
CREATE ROLE fieldbook LOGIN;
GRANT CONNECT ON DATABASE fieldbook TO fieldbook;
GRANT USAGE, CREATE ON SCHEMA public TO fieldbook;
```

Add version 1 to each of the following Secret Manager containers. Supply values directly to Secret Manager, for example with `gcloud secrets versions add SECRET_ID --project="$project" --data-file=/path/to/private-input`. Keep the secret values out of Terraform and shell history. Use synthetic values in staging.

| Secret container | Contents |
| --- | --- |
| `fieldbook-database-url` | `postgres://fieldbook:PASSWORD@localhost/fieldbook?host=/cloudsql/PROJECT:REGION:fieldbook-db&sslmode=disable` where PASSWORD is URL-encoded. SQLx opens the Unix socket and the connector encrypts its upstream connection. |
| `fieldbook-field-session-secret` | Session secret for this environment. |
| `fieldbook-field-diary-keyring` | Diary keyring for this environment. |
| `fieldbook-field-backup-recipients` | Public backup recipients. Keep recovery keys offline. |

## Release after migration

Run the following in a single shell with `vars`, `project`, and `region` set as above. Use this sequence for the first deployment and each upgrade. Run one release at a time per environment. A failed execution stops the sequence before the web service can change.

```sh
(
  set -eu
  terraform apply -var-file="$vars" -target=google_cloud_run_v2_job.migrate
  gcloud run jobs execute fieldbook-migrate --project="$project" --region="$region" --wait
  terraform apply -var-file="$vars"
)
```

Record the successful job execution and returned service URL. Run the requests below and record the status-only responses. Each should return HTTP 200 and `{"status":"ok"}`. A successful migration and readiness check exercise the mounted SQL socket. Confirm public access prevention on the bucket and denied anonymous object access with an invented marker. When exercising the media feature and recovery, use a separate backup destination with independent access and check that the same fixture file is available through two web instances.

```sh
service_url=$(terraform output -raw service_uri)
curl -fsS --max-time 5 -w '\nHTTP %{http_code}\n' "$service_url/healthz"
curl -fsS --max-time 5 -w '\nHTTP %{http_code}\n' "$service_url/readyz"
```

Configuration validation and a paid rehearsal are recorded independently. B0.3 serves status endpoints and applies the initial schema. Product media and backup behavior have their own acceptance cases.

[Cloud SQL sockets](https://docs.cloud.google.com/sql/docs/postgres/connect-run) · [Cloud Run image deployment](https://docs.cloud.google.com/run/docs/deploying) · [Cloud Run health checks](https://docs.cloud.google.com/run/docs/configuring/healthchecks) · [Bucket public access prevention](https://docs.cloud.google.com/storage/docs/public-access-prevention)
