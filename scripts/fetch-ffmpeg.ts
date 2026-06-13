#!/usr/bin/env bun
// Downloads static ffmpeg/ffprobe builds and places them where Tauri expects
// sidecar binaries: src-tauri/binaries/<name>-<target-triple><exe-suffix>.
//
// macOS: GPL static builds from https://ffmpeg.martin-riedl.de
// Windows: GPL static builds from https://github.com/BtbN/FFmpeg-Builds
//   (each zip contains bin/ffmpeg.exe and bin/ffprobe.exe)
//
// Run once after cloning, and again to update the bundled FFmpeg version.
// Usage: bun scripts/fetch-ffmpeg.ts [arm64|x64]   (default: host arch)
import { existsSync } from "node:fs";
import { mkdir, rm, copyFile, chmod, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const arch = process.argv[2] ?? process.arch; // "arm64" | "x64"
if (arch !== "arm64" && arch !== "x64") {
  console.error(`Unsupported arch: ${arch} (expected arm64 or x64)`);
  process.exit(1);
}

const os = process.platform; // "darwin" | "win32"
const TRIPLES: Record<string, string> = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "win32-x64": "x86_64-pc-windows-msvc",
};
const triple = TRIPLES[`${os}-${arch}`];
if (!triple) {
  console.error(`Unsupported platform: ${os}-${arch}`);
  process.exit(1);
}
const exe = os === "win32" ? ".exe" : "";
const destDir = join(import.meta.dir, "..", "src-tauri", "binaries");
await mkdir(destDir, { recursive: true });

/**
 * curl rather than fetch(): it ships everywhere this runs (macOS, Windows
 * 10+, GitHub runners), and martin-riedl.de has been seen stalling Bun's
 * fetch indefinitely on CI while curl downloads fine. Hard timeouts keep a
 * bad mirror from hanging a CI job for hours.
 */
async function download(url: string, to: string): Promise<void> {
  console.log(`↓ ${url}`);
  const p = Bun.spawn(
    [
      "curl",
      "-fsSL",
      "--retry",
      "5",
      // Retry transient HTTP errors too (BtbN republishes the rolling
      // `latest` release nightly, briefly 404ing its assets mid-update).
      "--retry-all-errors",
      "--retry-delay",
      "5",
      "--connect-timeout",
      "30",
      "--max-time",
      "600",
      "-o",
      to,
      url,
    ],
    { stdout: "inherit", stderr: "inherit" },
  );
  if ((await p.exited) !== 0) throw new Error(`curl failed for ${url}`);
}

/**
 * bsdtar ships with both macOS and Windows 10+ and extracts zips. On Windows
 * it must be the System32 binary by absolute path: in Git Bash environments
 * (GitHub Actions `shell: bash`) a plain `tar` resolves to GNU tar, which
 * parses `C:\…` as a remote host ("Cannot connect to C").
 */
async function extract(zip: string, into: string): Promise<void> {
  const tar = os === "win32" ? "C:\\Windows\\System32\\tar.exe" : "tar";
  const p = Bun.spawn([tar, "-xf", zip, "-C", into]);
  if ((await p.exited) !== 0) throw new Error(`tar failed on ${zip}`);
}

async function run(cmd: string[]): Promise<number> {
  const p = Bun.spawn(cmd, { stdout: "inherit", stderr: "inherit" });
  return await p.exited;
}

/**
 * Resolves the download URL + inner folder name of the newest BtbN
 * FFmpeg-Builds asset matching `suffix` (e.g. "win64-gpl"), via the GitHub
 * API. Deliberately skips the rolling `latest` release — its assets are
 * deleted and recreated nightly, so they 404 mid-republish (the reason a
 * direct `latest` download is flaky). The dated `autobuild-*` releases are
 * immutable, so the asset URL we return never 404s. The inner folder inside
 * each zip equals the asset name without ".zip".
 */
async function resolveBtbnAsset(
  suffix: string,
): Promise<{ url: string; inner: string }> {
  const headers: Record<string, string> = {
    "User-Agent": "tamp-fetch-ffmpeg",
    Accept: "application/vnd.github+json",
  };
  // Raises the 60/hr anonymous rate limit when a token is present (CI).
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }
  const res = await fetch(
    "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases?per_page=30",
    { headers },
  );
  if (!res.ok) throw new Error(`GitHub API ${res.status} listing BtbN releases`);
  const releases = (await res.json()) as Array<{
    tag_name: string;
    assets: Array<{ name: string; browser_download_url: string }>;
  }>;
  for (const rel of releases) {
    if (rel.tag_name === "latest") continue; // skip the volatile rolling release
    const asset = rel.assets.find((a) => a.name.endsWith(`-${suffix}.zip`));
    if (asset) {
      return {
        url: asset.browser_download_url,
        inner: asset.name.replace(/\.zip$/, ""),
      };
    }
  }
  throw new Error(`no ${suffix} asset in the 30 most recent BtbN releases`);
}

const dests = (["ffmpeg", "ffprobe"] as const).map((bin) => ({
  bin,
  dest: join(destDir, `${bin}-${triple}${exe}`),
}));
if (dests.every(({ dest }) => existsSync(dest))) {
  console.log("✓ sidecars already present, skipping (delete them to re-fetch)");
} else if (os === "darwin") {
  const riedlArch = arch === "arm64" ? "arm64" : "amd64";
  for (const { bin, dest } of dests) {
    if (existsSync(dest)) continue;
    const tmp = await mkdtemp(join(tmpdir(), "tamp-ffmpeg-"));
    const zip = join(tmp, `${bin}.zip`);
    await download(
      `https://ffmpeg.martin-riedl.de/redirect/latest/macos/${riedlArch}/release/${bin}.zip`,
      zip,
    );
    await extract(zip, tmp);
    await copyFile(join(tmp, bin), dest);
    await chmod(dest, 0o755);
    // Quarantine strip is best-effort (the attribute may be absent); the
    // ad-hoc codesign is required for the binary to run at all.
    await run(["xattr", "-d", "com.apple.quarantine", dest]);
    if ((await run(["codesign", "-fs", "-", dest])) !== 0) {
      throw new Error(`codesign failed for ${dest}`);
    }
    await rm(tmp, { recursive: true, force: true });
    console.log(`✓ ${dest}`);
  }
} else {
  // ARM64 Windows deliberately gets the x64 build: BtbN's winarm64 zips lack
  // libvpx (no WebM/VP9), and Windows 11 runs x64 binaries transparently via
  // emulation. Codec parity beats native speed; flip to winarm64 once it
  // ships libvpx.
  const { url, inner } = await resolveBtbnAsset("win64-gpl");
  const tmp = await mkdtemp(join(tmpdir(), "tamp-ffmpeg-"));
  const zip = join(tmp, "ffmpeg.zip");
  await download(url, zip);
  await extract(zip, tmp);
  for (const { bin, dest } of dests) {
    if (existsSync(dest)) continue;
    await copyFile(join(tmp, inner, "bin", `${bin}${exe}`), dest);
    console.log(`✓ ${dest}`);
  }
  await rm(tmp, { recursive: true, force: true });
}

for (const { dest } of dests) {
  const p = Bun.spawn([dest, "-version"], { stdout: "pipe" });
  const firstLine = (await new Response(p.stdout).text()).split("\n")[0];
  console.log(firstLine);
}
