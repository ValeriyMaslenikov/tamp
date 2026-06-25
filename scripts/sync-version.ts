#!/usr/bin/env bun
// Mirrors the version from package.json (the single source of version truth,
// which tauri.conf.json reads) into the Rust crate manifest + lockfile so the
// crate version never drifts behind the app version.
//
// Writes:
//   - src-tauri/Cargo.toml   the [package] version field
//   - src-tauri/Cargo.lock   the `tamp` package entry's version
//
// Dependency-free (Bun's built-in file APIs + targeted regexes); idempotent.
// Used two ways:
//   - stable flow: `bun run sync-version` after a changesets bump, committed.
//   - beta flow:   prerelease.yml patches package.json from the tag, then runs
//                  this in the ephemeral checkout (nothing committed/reverted).
//
// Usage: bun scripts/sync-version.ts
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const pkgPath = join(root, "package.json");
const cargoTomlPath = join(root, "src-tauri", "Cargo.toml");
const cargoLockPath = join(root, "src-tauri", "Cargo.lock");

const pkg = (await Bun.file(pkgPath).json()) as { version?: string };
const version = pkg.version;
if (typeof version !== "string" || version.length === 0) {
  console.error("package.json has no version");
  process.exit(1);
}

// Cargo.toml: rewrite only the [package] version line. It is the first
// `version = "…"` in the file (the [package] table opens the manifest), so an
// anchored, single-replacement regex won't touch dependency version specs.
const cargoToml = await Bun.file(cargoTomlPath).text();
const tomlRe = /^(version\s*=\s*)"[^"]*"/m;
if (!tomlRe.test(cargoToml)) {
  console.error("could not find the [package] version in Cargo.toml");
  process.exit(1);
}
const nextToml = cargoToml.replace(tomlRe, `$1"${version}"`);

// Cargo.lock: rewrite the version line inside the `tamp` package block only.
// Match `name = "tamp"` followed by its `version = "…"` line. The lockfile may
// use CRLF (Windows), so tolerate an optional CR before the newline.
const cargoLock = await Bun.file(cargoLockPath).text();
const lockRe = /(name = "tamp"\r?\nversion = )"[^"]*"/;
if (!lockRe.test(cargoLock)) {
  console.error('could not find the "tamp" entry in Cargo.lock');
  process.exit(1);
}
const nextLock = cargoLock.replace(lockRe, `$1"${version}"`);

let changed = false;
if (nextToml !== cargoToml) {
  await Bun.write(cargoTomlPath, nextToml);
  changed = true;
}
if (nextLock !== cargoLock) {
  await Bun.write(cargoLockPath, nextLock);
  changed = true;
}

console.log(
  changed
    ? `✓ synced Cargo.toml + Cargo.lock to ${version}`
    : `✓ Cargo.toml + Cargo.lock already at ${version}`,
);
