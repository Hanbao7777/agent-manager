# GitHub Actions pin ledger

Reviewed on 2026-07-16. Every non-local `uses:` reference under `.github/workflows` is pinned to the full commit SHA below. Dependabot may open monthly GitHub Actions update proposals, limited to five open pull requests; this repository does not auto-approve or auto-merge them.

## Reviewed pins

| Repository | Reviewed release/ref | Full commit SHA | Official source | Purpose |
| --- | --- | --- | --- | --- |
| `actions/checkout` | `v7.0.0` | `9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` | [release](https://github.com/actions/checkout/releases/tag/v7.0.0) | Check out repository contents; its manifest uses the supported Node.js 24 Action runtime. |
| `actions/setup-node` | `v7.0.0` | `820762786026740c76f36085b0efc47a31fe5020` | [release](https://github.com/actions/setup-node/releases/tag/v7.0.0) | Install the selected Node.js toolchain; its manifest uses the supported Node.js 24 Action runtime. |
| `actions/cache` | `v6.1.0` | `55cc8345863c7cc4c66a329aec7e433d2d1c52a9` | [release](https://github.com/actions/cache/releases/tag/v6.1.0) | Restore and save pnpm and Cargo caches. |
| `actions/upload-artifact` | `v7.0.1` | `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` | [release](https://github.com/actions/upload-artifact/releases/tag/v7.0.1) | Upload explicitly requested internal or release artifacts. |
| `actions/download-artifact` | `v8.0.1` | `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c` | [release](https://github.com/actions/download-artifact/releases/tag/v8.0.1) | Collect release artifacts before publishing. |
| `actions/labeler` | `v6.2.0` | `b8dd2d9be0f68b860e7dae5dae7d772984eacd6d` | [release](https://github.com/actions/labeler/releases/tag/v6.2.0) | Apply pull-request labels. |
| `actions/stale` | `v10.4.0` | `1e223db275d687790206a7acac4d1a11bd6fe629` | [release](https://github.com/actions/stale/releases/tag/v10.4.0) | Mark and close stale issues. |
| `pnpm/action-setup` | `v6.0.9` | `0ebf47130e4866e96fce0953f49152a61190b271` | [release](https://github.com/pnpm/action-setup/releases/tag/v6.0.9) | Install the pinned pnpm version. |
| `dtolnay/rust-toolchain` | `stable` ref | `4be7066ada62dd38de10e7b70166bc74ed198c30` | [commit](https://github.com/dtolnay/rust-toolchain/commit/4be7066ada62dd38de10e7b70166bc74ed198c30) | Install Rust; each use explicitly selects `stable` or exact `1.95.0` through `toolchain`. |
| `anthropics/claude-code-action` | `v1.0.175` | `1298632ce7736903d02a1435002705aa2a594a6c` | [release](https://github.com/anthropics/claude-code-action/releases/tag/v1.0.175) | Run the read-only Claude review workflow. |
| `softprops/action-gh-release` | `v3.0.2` | `3d0d9888cb7fd7b750713d6e236d1fcb99157228` | [release](https://github.com/softprops/action-gh-release/releases/tag/v3.0.2) | Publish the existing prerelease assets on tag workflows. |

## Workflow inventory

| Workflow and step | Before | After |
| --- | --- | --- |
| `claude.yml` / Checkout | `actions/checkout@v6` | `actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` |
| `claude.yml` / Run Claude | `anthropics/claude-code-action@v1` | `anthropics/claude-code-action@1298632ce7736903d02a1435002705aa2a594a6c` |
| `ci.yml` / frontend Checkout | `actions/checkout@v6` | `actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` |
| `ci.yml` / Setup Node.js | `actions/setup-node@v6` | `actions/setup-node@820762786026740c76f36085b0efc47a31fe5020` |
| `ci.yml` / Setup pnpm | `pnpm/action-setup@v6` | `pnpm/action-setup@0ebf47130e4866e96fce0953f49152a61190b271` |
| `ci.yml` / Cache pnpm store | `actions/cache@v5` | `actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
| `ci.yml` / backend Checkout | `actions/checkout@v6` | `actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` |
| `ci.yml` / Setup Rust | `dtolnay/rust-toolchain@stable` | `dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30` |
| `ci.yml` / Restore Cargo cache | `actions/cache/restore@v5` | `actions/cache/restore@55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
| `ci.yml` / Save Cargo cache | `actions/cache/save@v5` | `actions/cache/save@55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
| `labeler.yml` / label | `actions/labeler@v5` | `actions/labeler@b8dd2d9be0f68b860e7dae5dae7d772984eacd6d` |
| `macos-internal.yml` / Checkout | `actions/checkout@v4` | `actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` |
| `macos-internal.yml` / Setup Node.js | `actions/setup-node@v4` | `actions/setup-node@820762786026740c76f36085b0efc47a31fe5020` |
| `macos-internal.yml` / Setup Rust | `dtolnay/rust-toolchain@1.95.0` | `dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30` |
| `macos-internal.yml` / Upload artifact | `actions/upload-artifact@v4` | `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` |
| `release.yml` / Checkout | `actions/checkout@v6` | `actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` |
| `release.yml` / Setup Node.js | `actions/setup-node@v6` | `actions/setup-node@820762786026740c76f36085b0efc47a31fe5020` |
| `release.yml` / Setup Rust | `dtolnay/rust-toolchain@stable` | `dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30` |
| `release.yml` / Setup pnpm | `pnpm/action-setup@v6` | `pnpm/action-setup@0ebf47130e4866e96fce0953f49152a61190b271` |
| `release.yml` / Setup pnpm cache | `actions/cache@v5` | `actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9` |
| `release.yml` / Upload artifacts | `actions/upload-artifact@v7` | `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` |
| `release.yml` / Download artifacts | `actions/download-artifact@v8` | `actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c` |
| `release.yml` / Upload release assets | `softprops/action-gh-release@v3` | `softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228` |
| `stale.yml` / stale | `actions/stale@v10` | `actions/stale@1e223db275d687790206a7acac4d1a11bd6fe629` |
| `windows-internal.yml` / Checkout | `actions/checkout@v4` | `actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0` |
| `windows-internal.yml` / Setup Node.js | `actions/setup-node@v4` | `actions/setup-node@820762786026740c76f36085b0efc47a31fe5020` |
| `windows-internal.yml` / Setup Rust | `dtolnay/rust-toolchain@1.95.0` | `dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30` |
| `windows-internal.yml` / Upload artifact | `actions/upload-artifact@v4` | `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` |

## macOS runner gates

The default manual internal verification selects `macos-15`, which GitHub documents as a standard M1/arm64 runner. Setting the explicit `release_candidate_intel` dispatch input to `true` selects `macos-15-intel`, which GitHub documents as a standard Intel runner. Both labels appear in GitHub's standard public-repository runner table, whose use is free and unlimited for public repositories; neither is a larger-runner label. Source: [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories).

The `build_artifacts` input remains `false` by default. Artifact build, verification, and upload steps remain conditional on `build_artifacts`, while artifact-free Apple Silicon and Intel jobs retain the 15-minute job cap and 12-minute Rust check/test step caps.

## Local audit

Run `node scripts/ci/audit-action-pins.mjs`. The dependency-free check runs in the existing frontend CI job, scans every workflow YAML file, skips local `./` actions, and fails if any other `uses:` value is not pinned to exactly 40 hexadecimal characters.
