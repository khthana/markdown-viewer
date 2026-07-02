Status: ready-for-agent

# Distribution: cargo-dist release workflow + crates.io publish

## Parent

.scratch/mdview-v1/PRD.md

## What to build

Dual distribution — publishable via `cargo install` from crates.io, and as prebuilt binaries attached to GitHub Releases for users without a Rust toolchain, with builds triggered by tagging a release.

## Acceptance criteria

- [ ] `cargo-dist` configured (`cargo dist init`) targeting `x86_64-pc-windows-msvc` (primary), `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`
- [ ] `.github/workflows/release.yml` (cargo-dist generated): triggered on version tag push, builds and attaches binaries for all configured targets to a GitHub Release
- [ ] A separate job/step publishes to crates.io via `cargo publish`, distinct from the binary-attach step (so one channel can be re-run independently of the other)
- [ ] `Cargo.toml` has crate metadata required for crates.io publishing (description, license, repository, keywords)
- [ ] README documents both installation paths: `cargo install mdview` and downloading a binary from GitHub Releases
- [ ] Dry-run verification: tagging a test version locally and confirming the release workflow builds successfully in CI (actual publish to crates.io/GitHub Releases can be a manual final step, not required for this ticket's completion)

## Blocked by

- 01-project-scaffold-raw-pager-ci
