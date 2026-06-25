# Releasing tamp

tamp uses a **release-branch** model: features accumulate on a per-version
branch, get stabilized and beta-tested there, and then land on `main` in one
shot, where changesets cuts the tagged release. The sections below cover that
flow, plus the underlying automated/manual mechanics.

## Release-branch workflow (the normal path)

1. **Feature work** — each change is its own branch off `main`, carries a
   changeset in `.changeset/` (`bun changeset`), and is opened as a PR. Stacked
   PRs are fine (a feature off another feature).
2. **Cut the release branch** — `git checkout main && git checkout -b
   release/X.Y.Z`.
3. **Accumulate** — merge every branch slated for the release into it:
   `git merge --no-ff <feature>`. Stacked branches bring their bases along, so
   merging the stack tips is enough. Resolve any conflicts **here, once** —
   never on `main`.
4. **Stabilize & beta-test** — cut betas straight from the release branch by
   pushing `vX.Y.Z-beta.N` tags (see *Beta releases* below); the Beta workflow
   builds installers from the tagged commit. Fix forward on the branch and
   re-tag until it's green in CI and verified on-device.
5. **Land it** — open a PR from `release/X.Y.Z` → `main` and merge. Do **not**
   hand-edit `package.json`; the accumulated changesets carry the version
   intent.
6. **Ship** — on `main`, the Release workflow's changesets step opens/updates a
   **"chore: release"** PR (bumps `package.json` + `CHANGELOG.md`). Merging that
   PR tags `vX.Y.Z`, creates the GitHub Release, and the build matrix attaches
   the macOS DMG + Windows NSIS x64/arm64.
7. **Clean up** — delete `release/X.Y.Z` and the merged feature branches.

> Why a release branch: it's the single place to integrate and beta-test the
> whole next version together (the features are individually green, but their
> *combination* needs one coherent test surface), while `main` stays releasable
> and the changesets automation below is left untouched.

## Stable releases (changesets, automated)

1. Merged PRs accumulate changeset files in `.changeset/`.
2. The release workflow keeps a **"chore: release"** PR up to date (version
   bump in `package.json` — the single source of version truth, which
   `tauri.conf.json` reads — plus `CHANGELOG.md`). Merging it tags `vX.Y.Z`
   and creates the GitHub Release.
3. The `build` matrix then attaches: macOS DMG (Apple Silicon), Windows NSIS
   x64, and Windows NSIS arm64.

> **Keep the crate version in sync.** `package.json` is the source of truth, but
> `src-tauri/Cargo.toml`/`Cargo.lock` carry their own version. Run
> `bun run sync-version` after a version bump to mirror it into the crate (and
> commit the result). The Release workflow fails on `main` if the two drift, so
> a forgotten sync is caught before it ships.

## Beta releases (tag-driven, any branch)

Tag-push workflows run the workflow file **at the tagged commit**, so betas
work from feature branches without touching main — and the version now comes
**from the tag**, so there is no `package.json` edit and nothing to revert:

1. `git tag v0.3.0-beta.1 && git push origin v0.3.0-beta.1`
2. The **Beta release** workflow derives the version from the tag
   (`vX.Y.Z-beta.N` → `X.Y.Z-beta.N`), patches `package.json` + the crate
   version in its ephemeral checkout (`bun run sync-version`), builds, and
   creates a GitHub *prerelease* with the same three installers.

Because the bump happens only in the throwaway CI checkout, `main`/the branch
never carries a `-beta` version — the Release workflow's guard rejects one if it
somehow does, so a leftover `-beta.N` can never poison the next changesets bump.

## Manual fallback (no CI)

On a machine of the target OS:

```bash
bun install && bun scripts/fetch-ffmpeg.ts && bun tauri build
```

then upload from `src-tauri/target/release/bundle/{dmg,nsis}/` with
`gh release upload <tag> <file>`.

For a different arch than the host, pass `--target <triple>` to
`bun tauri build` and the arch (`arm64`/`x64`) to the fetch script; a Windows
arm64 cross-build also needs `rustup target add aarch64-pc-windows-msvc`.

Note: Windows installers (both x64 and arm64) bundle the x64 FFmpeg build —
BtbN's winarm64 zips lack libvpx (WebM), and Windows 11 runs x64 binaries
transparently. Flip `scripts/fetch-ffmpeg.ts` back to winarm64 once it ships
libvpx.

## Code signing

The macOS build is signed + notarized when the Apple secrets are configured (it
falls back to unsigned otherwise). Setup, the exact secrets, and the
open-source secret-safety model live in **[code-signing.md](code-signing.md)**.
