# ADR-0009: Two distribution channels, and the name we could not have

Status: Accepted
Implements: `.scratch/mdview-v1/issues/13-distribution-release-workflow.md`

## Context

The PRD asks for two ways to get the program: `cargo install` for people
who already have Rust, and a prebuilt binary from GitHub Releases for
everyone else, with tagging a version doing the work. It also left an
open question in Further Notes — *"Binary name proposed as `mdview`;
confirm availability on crates.io before publishing."*

That question now has an answer, and it is no.

## Decisions

- **The crate is `mdview-term`; the command stays `mdview`.** `mdview`
  was published to crates.io in 2020 by an unrelated browser-based tool
  and is not available. A separate `[[bin]]` keeps the installed command
  what the PRD intended, so the only thing that changes for a reader is
  the install line: `cargo install mdview-term`. The PRD's user story 31
  ("install via `cargo install mdview`") cannot be met literally and is
  met in substance. The README says so plainly rather than quietly
  shipping a different name.
- **Licence is MIT OR Apache-2.0**, the Rust ecosystem's default pairing,
  with both texts in the repo. `dist` bundles them and the README into
  every archive, so a downloaded binary carries its own terms.
- **`installers = []`.** `dist` offers shell and PowerShell installer
  scripts; the issue asks for downloadable binaries, and a `curl | sh`
  endpoint is a distribution channel of its own to maintain. Archives
  only, for now.
- **Four targets, not five.** Windows x86-64, both macOS architectures,
  and Linux x86-64 — what the issue names. `dist init` also offers
  aarch64 Linux; it is one line in `dist-workspace.toml` whenever
  someone asks.
- **crates.io publishing is a job of its own**
  (`.github/workflows/publish-crates-io.yml`), wired in as a `dist`
  custom publish job. The two channels fail and are re-run
  independently, which is what user story 35 asks for; the job also takes
  a `workflow_dispatch` with a tag, because re-running one channel by
  hand is the case that rationale describes. `host` has already attached
  the binaries by the time it runs, so a crates.io failure costs the
  announcement, not the release.
- **A missing `CARGO_REGISTRY_TOKEN` fails the job loudly.** `dist`
  skips the publish job outright for prereleases, so reaching it without
  a token can only happen on a real release — where going green having
  published nothing is the worst outcome available. The error names the
  fix, and the job can be re-run alone once the secret exists.
- **The publish job sets no `permissions:` block.** It is the one thing
  that looks like an obvious hardening and is not: a called workflow can
  only narrow *within* what the caller granted, and `dist`'s generated
  caller lists `id-token` and `packages` and nothing else — so every
  other scope is already `none`. Asking for `contents: read` on top is
  asking for more than the caller has, and the entire run fails before
  its first job.

## Consequences

- **The repository's Actions token must be Read and write.** `dist`'s
  jobs request `contents: write` to create the Release, and a job may not
  request more than the repository's default maximum. With the default
  left at read-only, every release run dies with `startup_failure` and no
  job logs at all — the failure gives you almost nothing to go on, so it
  is recorded here.
- **A tag must match the package version exactly.** `dist` resolves the
  tag against `Cargo.toml`, so `v0.1.0-rc.1` only means anything if the
  version *is* `0.1.0-rc.1`. Verifying the workflow with a prerelease tag
  therefore means bumping the version first, which is also what makes the
  crates.io job skip itself.
- Publishing to crates.io needs a `CARGO_REGISTRY_TOKEN` repository
  secret, which does not exist yet. Until it does, a real release will
  attach binaries and then fail the publish job by design.
- `ci.yml` moved to `actions/checkout@v6` to match what `dist` generates,
  so the repo has one version of that action rather than two.
- `release.yml` is generated. Edit `dist-workspace.toml` and re-run
  `dist generate`; `dist generate --check` fails CI-style if the two
  drift. Note the tool is now called `dist`, not `cargo dist` as the
  issue and the PRD both still say.

## Verification

The workflow was proven end to end on 2026-08-27: `v0.1.0-rc.1` built all
four targets, `host` attached twelve assets to a prerelease, the crates.io
job skipped itself as prereleases should, and the Windows archive was
downloaded and its `mdview.exe` run — `--version` and a piped render both
correct. The release and tag were then deleted and the version restored
to `0.1.0`.
