# Code signing (macOS Developer ID + notarization)

The release workflows (`.github/workflows/prerelease.yml` and `release.yml`) sign
and **notarize** the macOS build when the Apple secrets below are present. With
no secrets configured the macOS build falls back to the previous ad-hoc signing,
so the pipeline never breaks — signing is purely additive.

> Windows builds remain unsigned for now; this covers macOS only.

## How it's wired

On the **macOS** runner only, a step imports the Developer ID certificate into a
throwaway keychain, then `bun tauri build` signs the `.app` with
`APPLE_SIGNING_IDENTITY`, notarizes it with Apple, and staples the ticket. The
step is gated on `runner.os == 'macOS' && env.APPLE_SIGNING_IDENTITY != ''`, and
it only lives in the **tag-driven** (`prerelease.yml`) and **main/release**
(`release.yml`) workflows — never in the `pull_request` CI workflow.

## Secrets to create

Add these under **Settings → Secrets and variables → Actions → New repository
secret** (or, for the hardened setup, as **Environment** secrets — see below).

| Secret | What it is | How to get it |
|--------|------------|---------------|
| `APPLE_CERTIFICATE` | Base64 of the **Developer ID Application** cert exported as a `.p12` (includes the private key) | See *Exporting the certificate* below |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12` | You choose it at export time |
| `APPLE_SIGNING_IDENTITY` | The full identity string, e.g. `Developer ID Application: Your Name (ABCDE12345)` | `security find-identity -v -p codesigning` |
| `APPLE_ID` | Your Apple Developer account email | — |
| `APPLE_PASSWORD` | An **app-specific password** (not your real password) | [appleid.apple.com](https://appleid.apple.com) → Sign-In and Security → App-Specific Passwords |
| `APPLE_TEAM_ID` | Your 10-character Team ID | [developer.apple.com](https://developer.apple.com) → Membership |

### Exporting the certificate

1. **Keychain Access** → *My Certificates* → find **Developer ID Application:
   Your Name (TEAMID)**.
2. Right-click → **Export** → save as `Certificates.p12`, set an export password
   (this becomes `APPLE_CERTIFICATE_PASSWORD`).
3. Base64-encode it for the secret value:
   ```bash
   base64 -i Certificates.p12 | pbcopy   # now paste as APPLE_CERTIFICATE
   ```

## Preventing secret leaks (open-source safety)

- **Fork PRs never receive secrets.** Workflows triggered by `pull_request` from
  a fork run with empty `secrets.*` and a read-only token, so a contributor's PR
  — even one that edits a workflow to print the cert — has nothing to leak. This
  is the core guarantee.
- **Signing runs only on trusted triggers** (tag push, push to `main`), which can
  only come from commits already in this repo.
- **Never** use `pull_request_target` for building/signing — it exposes secrets
  to PR-controlled code.
- The CI workflow (`ci.yml`, which runs on `pull_request`) does **not** carry the
  Apple env and never signs — it only verifies unsigned builds.
- The import step uses an ephemeral keychain password (`github.run_id`), deletes
  the decoded `.p12` immediately, and never echoes secret values.

### Optional hardening: a protected Environment

For defense-in-depth against a *merged* malicious change, move the six secrets
into a GitHub **Environment** (Settings → Environments → New → `release`) with
**Required reviewers** and deployment branches limited to `main` / `v*` tags,
then add `environment: release` to the `build` jobs. Each signed run then needs a
human approval. Trade-off: every signed beta/release waits on that approval.

## Verifying a signed build

After the first signed beta ships, on a Mac:

```bash
codesign -dv --verbose=4 /Applications/Tamp.app      # Authority: Developer ID Application: …
spctl -a -vvv -t install /Applications/Tamp.app      # accepted; source=Notarized Developer ID
xcrun stapler validate /Applications/Tamp.app        # The validate action worked!
```

Once a signed + notarized build is confirmed, drop the Gatekeeper/quarantine
workaround from the README and the [Installing Tamp](wiki/Installing-Tamp) wiki
page.

## Notes

- `macOSPrivateApi: true` (the transparent panel) is fine for **notarization** —
  private-API use is an App Store *review* concern, not a notarization one.
- These changes can't be tested without the secrets and a real certificate; the
  proof is the next `v*-beta.N` tag after the secrets are set. If notarization
  rejects on entitlements, add a hardened-runtime entitlements file to the macOS
  bundle config and retry.
