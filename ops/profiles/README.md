# Runtime Profiles

Profile files in this folder provide explicit defaults for local/dev vs non-dev deployments.

- `dev.env`: fast local iteration defaults
- `non-dev.env`: staging/production-like defaults (fail-closed + real verifier path)

Example usage with Docker Compose:

```bash
docker compose --env-file ops/profiles/dev.env up --build
docker compose --env-file ops/profiles/non-dev.env up --build
```

Example usage with E2E script:

```bash
source ops/profiles/non-dev.env
./scripts/e2e_run.sh
```
