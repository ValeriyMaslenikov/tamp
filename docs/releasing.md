# Releasing tamp

## Stable releases (changesets, automated)

1. Merged PRs accumulate changeset files in `.changeset/`.
2. The release workflow keeps a **"chore: release"** PR up to date (version
   bump in `package.json` — the single source of version truth, which
   `tauri.conf.json` reads — plus `CHANGELOG.md`). Merging it tags `vX.Y.Z`
   and creates the GitHub Release.
3. The `build` matrix then attaches: macOS DMG (Apple Silicon), Windows NSIS
   x64, and Windows NSIS arm64.

## Beta releases (manual tag, any branch)

Tag-push workflows run the workflow file **at the tagged commit**, so betas
work from feature branches without touching main:

1. On the branch, set a pre-release version in `package.json`
   (e.g. `0.3.0-beta.1`) and commit.
2. `git tag v0.3.0-beta.1 && git push origin v0.3.0-beta.1`
3. The **Beta release** workflow creates a GitHub *prerelease* with the same
   three installers.
4. Before merging the branch, revert the version-bump commit — changesets
   computes the stable version from `package.json`, and a leftover `-beta.N`
   would corrupt the next bump.

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
