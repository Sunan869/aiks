# AIKS V4.2 Customized SiYuan Runtime Packaging Plan

**Goal:** Replace the official SiYuan installer download path with a reproducible AIKS runtime built from `Sunan869/aiks-siyuan`, while pinning and validating the exact frontend/kernel source identity used by AIKS Desktop.

**Architecture:** `aiks-siyuan` builds the customized frontend and Windows kernel from one checkout and publishes a runtime ZIP. The AIKS repository consumes only that runtime ZIP, validates its manifest and SHA-256, preserves the separately packaged AIKS bridge plugin, and bundles the verified runtime through Tauri.

## Locked Runtime Identity

- SiYuan base: `3.8.3`
- Upstream commit: `8641553a1f07374001902d3ce773285db1292b2d`
- Fork repository: `Sunan869/aiks-siyuan`
- Profile: `aiks-embedded`
- Platform: `windows-x64`
- Bridge protocol: `1`

The fork commit and runtime archive SHA-256 are filled only after the runtime build/release succeeds.

## Task 1 — Fork runtime artifact

In `Sunan869/aiks-siyuan`:

- Build the frontend with `SIYUAN_PROFILE=aiks-embedded`.
- Build `SiYuan-Kernel.exe` from the same checkout with `fts5 sqlcipher` and production mode.
- Package only `kernel`, `stage`, `appearance`, and `guide`.
- Add `aiks-runtime.json` containing Workbench version, base version, upstream commit, fork commit, profile, platform, and bridge protocol.
- Produce a SHA-256 sidecar.
- Smoke-test the kernel before publishing the artifact.

## Task 2 — Stable fork release asset

- Publish the verified runtime ZIP from the fork as an AIKS-specific GitHub Release asset.
- Do not use temporary GitHub Actions artifact URLs as the AIKS version lock.
- Use an AIKS-specific runtime tag independent from upstream SiYuan release tags.

## Task 3 — Main-repo runtime lock

Update `siyuan.version` to pin:

- runtime repository
- runtime release tag
- runtime asset
- runtime SHA-256
- fork commit
- upstream commit
- profile
- platform
- bridge protocol

Keep `version=3.8.3` for compatibility with existing runtime diagnostics.

## Task 4 — Runtime setup

Rewrite `scripts/setup-siyuan.ps1` so it:

1. downloads the runtime ZIP from the pinned `Sunan869/aiks-siyuan` release;
2. verifies SHA-256 before extraction;
3. validates `aiks-runtime.json` against `siyuan.version`;
4. validates `kernel`, `stage`, `appearance`, and `guide`;
5. preserves `resources/siyuan/data/plugins/aiks-bridge` from the AIKS repository;
6. replaces only the generated runtime directories/files;
7. writes the existing runtime version marker for compatibility;
8. runs a kernel smoke test.

No official `siyuan-note/siyuan` installer may be downloaded by the V4.2 setup path.

## Task 5 — Release validation

Update `scripts/build-release.ps1` so a previously prepared runtime is accepted only when both:

- the version marker matches, and
- `aiks-runtime.json` matches all pinned identity fields.

Then verify:

- frontend tests/build
- Rust fmt/clippy/tests
- Windows desktop compile
- Tauri bundle resource validation
- no accidental deletion of the AIKS bridge plugin
- runtime source identity visible in diagnostics
