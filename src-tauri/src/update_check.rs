//! The opt-in GitHub update check.
//!
//! A single GET to the public releases list (`/releases`, so prereleases are
//! included — tamp is pre-1.0 and beta testers want newer betas) tells us the
//! highest published version. If it's strictly newer than the installed build,
//! the frontend shows one gentle, dismissible card. This is the ONLY outbound
//! request the app makes and it sends nothing about the user: it's a pull to a
//! public API. The enabled flag is checked on the frontend, not here, so the
//! command stays trivially testable; a failed check returns `Err` and the
//! frontend silently ignores it (never an error toast).

use serde::Serialize;
use tauri::AppHandle;

/// The GitHub releases endpoint for the list (newest first), capped so we fetch
/// a useful window of recent releases without paging.
const RELEASES_URL: &str =
    "https://api.github.com/repos/ValeriyMaslenikov/tamp/releases?per_page=20";

/// What the frontend needs to render the "update available" card. `notes` is
/// the release body (markdown), shown/linked as "What's new".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub notes: Option<String>,
}

/// One release as we read it off the GitHub JSON. Only the fields we need;
/// drafts are skipped, prereleases are kept.
#[derive(Debug, Clone, serde::Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
}

/// Parse a release tag (`v0.3.0-beta.7` or `0.3.0-beta.7`) into a semver
/// version, tolerating a single leading `v`. Returns `None` for tags that
/// aren't semver so a stray tag can't break the whole check. Pure.
fn parse_tag(tag: &str) -> Option<semver::Version> {
    let stripped = tag.strip_prefix('v').unwrap_or(tag);
    semver::Version::parse(stripped).ok()
}

/// Among `tags`, pick the highest semver version (prereleases included, ordered
/// per semver: `0.3.0-beta.7` < `0.3.0`) and return it only when it is strictly
/// greater than `installed`; otherwise `None`. Pure — no HTTP — so the version
/// logic is unit-tested without the network. Non-semver tags are ignored.
fn newer_version<'a>(
    tags: impl IntoIterator<Item = &'a str>,
    installed: &semver::Version,
) -> Option<semver::Version> {
    let highest = tags.into_iter().filter_map(parse_tag).max()?;
    (highest > *installed).then_some(highest)
}

/// Pick the newest release strictly newer than `installed`, ignoring drafts and
/// non-semver tags. Returns the matching release's display info. Pure over an
/// already-fetched list, so the decision is testable without the network.
fn choose_update(releases: &[Release], installed: &semver::Version) -> Option<UpdateInfo> {
    let live: Vec<&Release> = releases.iter().filter(|r| !r.draft).collect();
    let tags = live.iter().map(|r| r.tag_name.as_str());
    let chosen = newer_version(tags, installed)?;
    // Map the winning version back to the release that carried its tag for the
    // url/notes (parse again so `v`-prefixed and bare tags both match).
    live.iter()
        .find(|r| parse_tag(&r.tag_name).as_ref() == Some(&chosen))
        .map(|r| UpdateInfo {
            version: chosen.to_string(),
            url: r.html_url.clone(),
            notes: r.body.clone(),
        })
}

/// Blocking GET + parse of the releases list. Separated from the command so the
/// async wrapper just hands it the installed version. GitHub requires a
/// `User-Agent`; we send a self-identifying one and nothing else.
///
// manual: with the feature on, a real launch hits the live endpoint and finds
// the latest published release.
fn fetch_and_choose(
    installed: semver::Version,
    user_agent: &str,
) -> Result<Option<UpdateInfo>, String> {
    let body = ureq::get(RELEASES_URL)
        .header("User-Agent", user_agent)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("update check request failed: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("update check response unreadable: {e}"))?;
    let releases: Vec<Release> =
        serde_json::from_str(&body).map_err(|e| format!("update check parse failed: {e}"))?;
    Ok(choose_update(&releases, &installed))
}

/// Ask GitHub whether a newer tamp exists. Returns `Some(UpdateInfo)` when the
/// highest non-draft release (prereleases included) is strictly newer than the
/// installed build, `None` when up to date, and `Err(short msg)` on any
/// network/parse failure (the frontend silently ignores it — never a toast).
///
/// Does NOT read the enabled setting: the frontend only calls this when the
/// user has opted in, which keeps the command simple and side-effect-free.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let installed = semver::Version::parse(&app.package_info().version.to_string())
        .map_err(|e| format!("installed version is not semver: {e}"))?;
    let user_agent = format!("tamp/{installed} update-check");
    // The blocking HTTP call must not run on the async runtime's worker threads.
    tauri::async_runtime::spawn_blocking(move || fetch_and_choose(installed, &user_agent))
        .await
        .map_err(|e| format!("update check task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn parse_tag_strips_leading_v_and_keeps_prerelease() {
        assert_eq!(parse_tag("v0.3.0-beta.7"), Some(v("0.3.0-beta.7")));
        assert_eq!(parse_tag("0.3.0-beta.7"), Some(v("0.3.0-beta.7")));
        assert_eq!(parse_tag("v1.2.3"), Some(v("1.2.3")));
    }

    #[test]
    fn parse_tag_rejects_non_semver() {
        assert_eq!(parse_tag("nightly"), None);
        assert_eq!(parse_tag("v1.2"), None);
        assert_eq!(parse_tag(""), None);
    }

    #[test]
    fn picks_highest_across_the_list_including_prereleases() {
        // The newest published is a prerelease beta; it must win over the older
        // stable releases in the same list.
        let tags = ["v0.1.0", "v0.2.0", "v0.3.0-beta.7", "v0.2.5"];
        assert_eq!(
            newer_version(tags, &v("0.2.0")),
            Some(v("0.3.0-beta.7")),
            "newest prerelease in the list should be chosen"
        );
    }

    #[test]
    fn prerelease_is_newer_than_older_stable() {
        // 0.3.0-beta.7 > 0.2.0 (different release line, higher base version).
        assert_eq!(
            newer_version(["v0.3.0-beta.7"], &v("0.2.0")),
            Some(v("0.3.0-beta.7"))
        );
    }

    #[test]
    fn stable_is_newer_than_its_own_prerelease() {
        // 0.3.0 > 0.3.0-beta.7 (semver: release outranks its prereleases).
        assert_eq!(
            newer_version(["v0.3.0"], &v("0.3.0-beta.7")),
            Some(v("0.3.0"))
        );
        // And a build still on the beta is NOT newer than that same beta.
        assert_eq!(newer_version(["v0.3.0-beta.7"], &v("0.3.0-beta.7")), None);
    }

    #[test]
    fn none_when_installed_is_up_to_date_or_ahead() {
        assert_eq!(newer_version(["v0.2.0", "v0.1.0"], &v("0.2.0")), None);
        assert_eq!(newer_version(["v0.2.0"], &v("0.3.0")), None);
    }

    #[test]
    fn ignores_non_semver_tags_without_breaking() {
        let tags = ["nightly", "v0.3.0", "latest"];
        assert_eq!(newer_version(tags, &v("0.2.0")), Some(v("0.3.0")));
    }

    fn release(tag: &str, draft: bool) -> Release {
        Release {
            tag_name: tag.to_string(),
            html_url: format!("https://example.test/{tag}"),
            body: Some(format!("notes for {tag}")),
            draft,
        }
    }

    #[test]
    fn choose_update_skips_drafts_and_maps_url_and_notes() {
        let releases = vec![
            release("v0.4.0", true), // draft: ignored even though it's highest
            release("v0.3.0-beta.7", false),
            release("v0.2.0", false),
        ];
        let got = choose_update(&releases, &v("0.2.0")).unwrap();
        assert_eq!(got.version, "0.3.0-beta.7");
        assert_eq!(got.url, "https://example.test/v0.3.0-beta.7");
        assert_eq!(got.notes.as_deref(), Some("notes for v0.3.0-beta.7"));
    }

    #[test]
    fn choose_update_returns_none_when_up_to_date() {
        let releases = vec![release("v0.2.0", false), release("v0.1.0", false)];
        assert_eq!(choose_update(&releases, &v("0.2.0")), None);
    }

    #[test]
    fn choose_update_ignores_a_draft_newer_than_everything() {
        // Only a draft is newer than installed → nothing to offer.
        let releases = vec![release("v0.9.0", true), release("v0.2.0", false)];
        assert_eq!(choose_update(&releases, &v("0.2.0")), None);
    }

    #[test]
    fn release_json_deserializes_with_missing_optional_fields() {
        // A real GitHub release row with no body and the draft flag present.
        let json = r#"[
            {"tag_name":"v0.3.0","html_url":"https://h/0.3.0","draft":false},
            {"tag_name":"v0.2.0","html_url":"https://h/0.2.0","body":"hi","draft":false}
        ]"#;
        let releases: Vec<Release> = serde_json::from_str(json).unwrap();
        let got = choose_update(&releases, &v("0.2.0")).unwrap();
        assert_eq!(got.version, "0.3.0");
        assert_eq!(got.notes, None);
    }
}
