# Windows SiYuan startup regression — 2026-09-22

The service-local profile is canonicalized for native path/ownership checks. Rust uses a Windows verbatim prefix for canonical paths. Passing that workspace directly to the pinned SiYuan kernel causes block-tree database initialization to exit with `unable to open database file` and a filename/directory syntax error. The old plain-workspace startup did not pass that prefix.

## Evidence before the fix

At test-only commit `0f793335c9906e3c7a73fd164f61d66451440cf0`, Windows Actions run `35726139369` used the unmodified production bootstrap and the real runtime selected by `siyuan.version` (not a placeholder or HTTP mock). A new plain workspace reached READY; a new canonical workspace failed in `blocktree.go:95` with the same filename syntax error. Both cases used temporary profiles with spaces and Chinese characters. The runtime and build were identical across both cases.

The kernel constructs its SQLite DSN from the block-tree database path plus query options. A verbatim path includes `?`, which conflicts with the driver's query-string delimiter. This is a path interoperability bug at the AIKS-to-SiYuan boundary, not missing model configuration or proof of damaged user knowledge.

## Targeted change

Only the paths in `BootstrapConfig::runtime_config` which are passed to SiYuan are simplified. Drive and UNC spellings are handled separately, and an ordinary spelling is used only after it resolves to the same existing canonical target. Unknown/device paths, missing paths, and non-Windows paths are preserved. No global string replacement, data-root migration, deletion or ownership/reparse-check removal is involved.

The original Rust data root is retained for ownership, logs and runtime metadata. The pinned kernel/runtime archive is unchanged. The optional real-Windows smoke tests are explicitly run by `siyuan-windows-startup.yml`; normal test runs do not silently download a kernel.

After the fix, verify the real-kernel workflow and normal Core/Service/Desktop quality gates at the exact new commit. This document does not claim a GREEN result in advance.

## Local retest

Exit AIKS normally, update `feature/aiks-service-extraction`, then run `scripts/dev.ps1` from the repository root. Keep the existing service-local workspace and databases. No reset, administrator elevation, manual SQLite table operation, or manual pandoc installation is part of this fix.
