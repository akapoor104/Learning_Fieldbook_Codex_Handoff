# Render Blueprint

The family can enable a paid staging rehearsal using the root `render.yaml`. Use an invented household and staging-only credentials.

1. Build and publish the shared release as described in the [Google runbook](../google/README.md#publish-the-shared-release). Put the complete image reference pinned by digest in `services[0].image.url`. Use the identical reference for home Compose and Google Cloud.
2. In the Render workspace, add an Artifact Registry credential named `fieldbook-release-reader`. Grant the credential's service account Artifact Registry Reader on the release repository. Store the credential in Render.
3. Choose **New → Blueprint** and select the repository and release branch. Verify that the web service and PostgreSQL 16 database are both in Oregon, the disk mounts at `/var/data/fieldbook`, and the database has no external allowlist entries.
4. Supply `FIELD_PUBLIC_ORIGIN` and `FIELD_RP_ID` for the staging HTTPS hostname and supply the keyring and backup recipients in the Blueprint setup form. Render generates the session secret and injects the internal database URL. Leave outside AI off.
5. In the first deploy log, confirm that `fieldbook migrate` succeeded. With `FIELD_PUBLIC_ORIGIN` set to the staging origin, run the two requests below. Each should return HTTP 200 and `{"status":"ok"}`. In the Dashboard Shell for the running service, check that UID 10001 can write a fictional marker on the mounted disk. Confirm that the marker survives a redeploy of the same digest.

```sh
curl -fsS --max-time 5 -w '\nHTTP %{http_code}\n' "$FIELD_PUBLIC_ORIGIN/healthz"
curl -fsS --max-time 5 -w '\nHTTP %{http_code}\n' "$FIELD_PUBLIC_ORIGIN/readyz"
```

For later releases, update the digest in the Blueprint and confirm the migration ran before the service was replaced. Use the running service for disk verification. The pre-deploy process only needs the database.

The CI schema check validates the Blueprint structure. When a staging workspace is available, `render blueprints validate render.yaml` also checks provider semantics and workspace conflicts without creating resources. Record that command and the first paid deploy as separate observations.

[Blueprint reference](https://render.com/docs/blueprint-spec) · [Image credentials](https://render.com/docs/deploying-an-image) · [Persistent disks](https://render.com/docs/disks)
