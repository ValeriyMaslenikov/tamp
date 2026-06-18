# FAQ & Troubleshooting

## FAQ

### Will Tamp ever hand me a file over my size limit?
No. Tamp plans the bitrate to land *under* your target and verifies the result,
re-encoding if it overshot. If a target is genuinely impossible for a clip, it
tells you instead of producing a too-big or unwatchable file — see
["Target too small"](#target-too-small).

### Does Tamp upload my videos anywhere?
No. All encoding happens locally with a bundled FFmpeg. There's no telemetry and
no account. The only optional network request is the GitHub update check, and it
sends nothing about you. See [Privacy](How-It-Works-and-Privacy#privacy--your-data).

### Does it work offline?
Yes — compression is fully local. Only the optional update check needs the
internet, and it's off by default.

### Where does the compressed file go?
Right next to the original, with a `(tamped …)` name. Optionally it's also
copied to your clipboard. See
[Output files](Converted-History-and-Output#where-files-are-saved).

### My output isn't in the Videos list — where is it?
That's by design: Tamp hides its own `(tamped …)` outputs from the recordings
list while the original exists, so the list stays "things to compress." Find
every output in the **[Converted](Converted-History-and-Output)** tab.

### Discord/Slack still rejects my file — why?
Make sure your preset's **Target MB** is at or under the platform's limit (e.g.
Discord's free limit is 10 MB; the built-in preset already targets that). If the
file is fine but too long to look good that small,
[split it into parts](Presets-and-Splitting#splitting-into-parts).

### Can I export more than one format from a single recording?
Yes — run it through several presets. But if **Move original to Trash** is on,
the original is gone after the first conversion, so Tamp allows only one. Turn
that toggle off to keep the original and export multiple formats. See
[Behavior](Preferences-and-Shortcuts#behavior).

### Where did my original recording go?
If **Move original to Trash** is on, Tamp moved it to the Trash (recoverable)
after compressing. The compressed copy stays in your
[history](Converted-History-and-Output).

### Why is re-compressing the same video instant?
Tamp reuses an existing output when the input and settings match (a fingerprint
in the file name). Change any setting to force a fresh encode. See
[reuse](Converted-History-and-Output#reuse-re-clicking-is-instant).

### Is there a Linux build?
Not yet. The internal platform layer is ready, but Linux needs a clipboard/tray
strategy and a CI target. Today's builds are macOS (Apple Silicon) and Windows.

### Is there a Dock / taskbar window?
No. Tamp lives only in the menu bar (macOS) or system tray (Windows). Open it
with the icon or the toggle shortcut.

---

## Troubleshooting

### SmartScreen warns me on Windows
The build is unsigned. Click **More info → Run anyway**. It installs per-user,
no admin needed. See [Installing on Windows](Installing-Tamp#windows).

### macOS says the app can't be opened / is from an unidentified developer
The build is ad-hoc signed (not notarized). **Right-click the app → Open →
Open**, or run `xattr -dr com.apple.quarantine /Applications/tamp.app`. See
[Installing on macOS](Installing-Tamp#macos-apple-silicon).

### Nothing shows up in the Videos list
- Tamp only lists files in your **watched folders**. Point it at wherever your
  recorder saves: [Watched folders](Preferences-and-Shortcuts#watched-folders).
- If you see a **"couldn't read a folder"** notice, that folder is offline or
  permission-denied (common for network shares) — it'll reappear once reachable.
- Your finished outputs live in the **[Converted](Converted-History-and-Output)**
  tab, not the Videos list.

### "Target too small"
A long or high-resolution clip may not fit your target even at minimum quality.
Tamp tells you rather than producing mush. Options: lower the **Max FPS** or
downscale (**Max width** / **Scale %**) in the preset, pick a **bigger target**,
or [split it into parts](Presets-and-Splitting#splitting-into-parts) so each part
fits.

### Encoding is slow or quality looks lower than expected
With **Use GPU encoder** on, Tamp prefers the fast hardware encoder. For tight
targets it switches to slower, more precise two-pass software encoding — that
trade-off is intentional. You can toggle the encoder in
[Behavior](Preferences-and-Shortcuts#behavior). See
[hardware vs software](How-It-Works-and-Privacy#hardware-vs-software-encoding).

### "File no longer exists" / can't play or reveal
The file was moved or deleted outside Tamp after it was recorded/converted.
Re-record or re-convert as needed.

### The stale-recording warning never appears (or I want it off)
It rides on desktop notifications. If notifications are off, Preferences shows a
recovery card (**Enable notifications** / **Open System Settings**). To disable
the warning entirely, set its threshold to **0** under
[Shortcuts](Preferences-and-Shortcuts#stale-recording-warning).

### Reading the logs & reporting a bug
Tamp keeps rotating local logs with every FFmpeg command and error. Open the
folder via **tray menu → Open Logs**. When filing an issue at
[github.com/ValeriyMaslenikov/tamp/issues](https://github.com/ValeriyMaslenikov/tamp/issues),
include your OS, the app version (bottom of Preferences), what you did, and the
relevant log lines.
