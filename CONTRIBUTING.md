# Contributing

Thanks for contributing to AstraGraph.

## Development setup

Core services live in `ASTRAGRAPH/` (Rust + Python + TypeScript).

Quick checks:

```bash
cd ASTRAGRAPH
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pytest tests/integration -q
```

End-to-end demo gate:

```bash
cd ASTRAGRAPH
./scripts/e2e_run.sh
```

## Pull requests

- Keep changes focused.
- Add/adjust tests when behavior changes.
- Prefer deterministic fixtures for E2E.
- Follow contribution tracks:
  - `docs/community_contribution_track.md`
  - `docs/public_roadmap.md`
