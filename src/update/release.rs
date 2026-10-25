//! What a GitHub release says, and every decision that needs nothing but it.
//!
//! Kept free of IO so the rules that decide whether Probe offers (and which
//! file it would install) are unit-tested without a network or a release.

use serde::Deserialize;

/// Name of the checksum file the release workflow publishes next to the
/// binaries, in `sha256sum` output format.
pub const SUMS_ASSET: &str = "SHA256SUMS";

/// The automatic startup check runs at most this often.
pub const CHECK_INTERVAL_SECONDS: i64 = 24 * 60 * 60;

/// The subset of `GET /repos/{owner}/{repo}/releases/latest` Probe reads.
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

pub type Version = (u64, u64, u64);

/// Parse `X.Y.Z` or `vX.Y.Z`. Anything else — pre-release suffixes, build
/// metadata, a missing component — is `None`, which means "never offered".
pub fn parse_version(text: &str) -> Option<Version> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.split('.');
    let version = (
        number(parts.next()?)?,
        number(parts.next()?)?,
        number(parts.next()?)?,
    );
    if parts.next().is_some() {
        return None;
    }
    Some(version)
}

fn number(text: &str) -> Option<u64> {
    // `u64::from_str` accepts a leading `+`; a version component should not.
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The release asset built for `os`/`arch` (as in `std::env::consts`).
///
/// Only exact pairs the release workflow builds map to a name; any other
/// platform gets `None` and is never offered an update.
pub fn asset_name_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("probe-linux-x86_64"),
        ("windows", "x86_64") => Some("probe-windows-x86_64.exe"),
        _ => None,
    }
}

/// The release asset for the platform this binary was compiled for.
pub fn this_build_asset_name() -> Option<&'static str> {
    asset_name_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Whether `release` is worth offering to a build at `current_version` that
/// installs `asset_name`: it must be strictly newer and carry both that
/// build's binary and the checksum file. Otherwise there is nothing safe to
/// install, so the user is not told about it.
pub fn worth_offering(release: &Release, current_version: &str, asset_name: Option<&str>) -> bool {
    let (Some(latest), Some(current)) = (
        parse_version(&release.tag_name),
        parse_version(current_version),
    ) else {
        return false;
    };
    let Some(asset_name) = asset_name else {
        return false;
    };
    latest > current && release.asset(asset_name).is_some() && release.asset(SUMS_ASSET).is_some()
}

/// The lowercase hex sha256 for `asset_name` in `sha256sum` output
/// (`<hex>  <name>`, or `<hex> *<name>` for binary mode).
pub fn expected_sha256(sums: &str, asset_name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name);
        let valid_hash = hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
        (name == asset_name && valid_hash).then(|| hash.to_ascii_lowercase())
    })
}

/// Whether the automatic check should run. A clock that went backwards
/// counts as due rather than silencing the check until it catches up.
pub fn check_due(last_checked_at: Option<i64>, now: i64) -> bool {
    match last_checked_at {
        None => true,
        Some(last) => now < last || now - last >= CHECK_INTERVAL_SECONDS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX: &str = "probe-linux-x86_64";

    fn release(tag: &str, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.to_owned(),
            body: None,
            html_url: format!("https://github.com/neisep/Probe/releases/tag/{tag}"),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).to_owned(),
                    browser_download_url: format!("https://example.com/{name}"),
                    size: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn parses_plain_and_v_prefixed_versions() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v0.10.0"), Some((0, 10, 0)));
    }

    #[test]
    fn rejects_versions_that_are_not_exactly_three_numbers() {
        for junk in [
            "",
            "v",
            "1.2",
            "1.2.3.4",
            "v1.2.3-rc1",
            "1.2.3+build",
            "+1.2.3",
            "1.+2.3",
            "1..3",
            "latest",
            "V1.2.3",
        ] {
            assert_eq!(parse_version(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn compares_numerically_not_lexically() {
        let current = "0.9.9";
        assert!(worth_offering(
            &release("v0.10.0", &[LINUX, SUMS_ASSET]),
            current,
            Some(LINUX)
        ));
    }

    #[test]
    fn same_or_older_release_is_not_offered() {
        let assets = [LINUX, SUMS_ASSET];
        assert!(!worth_offering(
            &release("v0.2.0", &assets),
            "0.2.0",
            Some(LINUX)
        ));
        assert!(!worth_offering(
            &release("v0.1.9", &assets),
            "0.2.0",
            Some(LINUX)
        ));
    }

    #[test]
    fn unparsable_tag_is_not_offered() {
        let release = release("nightly", &[LINUX, SUMS_ASSET]);
        assert!(!worth_offering(&release, "0.1.0", Some(LINUX)));
    }

    #[test]
    fn release_without_this_builds_binary_or_checksums_is_not_offered() {
        assert!(!worth_offering(
            &release("v9.0.0", &[SUMS_ASSET]),
            "0.1.0",
            Some(LINUX)
        ));
        assert!(!worth_offering(
            &release("v9.0.0", &[LINUX]),
            "0.1.0",
            Some(LINUX)
        ));
        // A platform with no asset name at all (e.g. aarch64) gets nothing.
        assert!(!worth_offering(
            &release("v9.0.0", &[LINUX, SUMS_ASSET]),
            "0.1.0",
            None
        ));
    }

    #[test]
    fn only_built_platforms_have_an_asset_name() {
        assert_eq!(asset_name_for("linux", "x86_64"), Some(LINUX));
        assert_eq!(
            asset_name_for("windows", "x86_64"),
            Some("probe-windows-x86_64.exe")
        );
        assert_eq!(asset_name_for("linux", "aarch64"), None);
        assert_eq!(asset_name_for("macos", "x86_64"), None);
        assert_eq!(asset_name_for("windows", "aarch64"), None);
    }

    #[test]
    fn reads_the_hash_for_the_named_asset_only() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!("{a}  probe-windows-x86_64.exe\n{b} *{LINUX}\n");
        assert_eq!(expected_sha256(&sums, LINUX), Some("b".repeat(64)));
        assert_eq!(
            expected_sha256(&sums, "probe-windows-x86_64.exe"),
            Some(a.clone())
        );
        assert_eq!(expected_sha256(&sums, "probe-linux-x86"), None);
    }

    #[test]
    fn ignores_malformed_checksum_lines() {
        let short = "a".repeat(63);
        let not_hex = "g".repeat(64);
        let sums = format!("{short}  {LINUX}\n{not_hex}  {LINUX}\n\n{LINUX}\n");
        assert_eq!(expected_sha256(&sums, LINUX), None);
    }

    #[test]
    fn check_is_due_after_a_day_or_when_never_run() {
        let now = 1_000_000;
        assert!(check_due(None, now));
        assert!(!check_due(Some(now - CHECK_INTERVAL_SECONDS + 1), now));
        assert!(check_due(Some(now - CHECK_INTERVAL_SECONDS), now));
        // Clock moved backwards.
        assert!(check_due(Some(now + 60), now));
    }

    #[test]
    fn parses_the_github_release_shape() {
        let json = r####"{
            "tag_name": "v0.2.0",
            "name": "v0.2.0",
            "html_url": "https://github.com/neisep/Probe/releases/tag/v0.2.0",
            "body": "### Added\n\n- thing",
            "draft": false,
            "assets": [
                {"name": "probe-linux-x86_64", "size": 12345,
                 "browser_download_url": "https://github.com/neisep/Probe/releases/download/v0.2.0/probe-linux-x86_64",
                 "content_type": "application/octet-stream"}
            ]
        }"####;
        let release: Release = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v0.2.0");
        assert_eq!(release.body.as_deref(), Some("### Added\n\n- thing"));
        assert_eq!(release.asset(LINUX).map(|a| a.size), Some(12345));
    }

    #[test]
    fn missing_body_and_assets_default() {
        let json = r#"{"tag_name": "v1.0.0", "html_url": "https://x", "body": null}"#;
        let release: Release = serde_json::from_str(json).unwrap();
        assert!(release.body.is_none());
        assert!(release.assets.is_empty());
    }
}
