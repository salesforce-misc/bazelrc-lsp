use once_cell::sync::Lazy;

use crate::{bazel_flags::load_packaged_bazel_flag_collection, file_utils::get_workspace_path};
use std::{env, fs, path::Path};

#[derive(Debug, PartialEq)]
struct BazelVersion {
    major: i16,
    minor: i16,
    patch: i16,
    fork_owner: Option<String>,
    pre_release: Option<String>,
}

type BazelVersionTuple = (i16, i16, i16, Option<String>, Option<String>);

impl BazelVersion {
    fn as_tuple(&self) -> BazelVersionTuple {
        (
            self.major,
            self.minor,
            self.patch,
            self.fork_owner.clone(),
            self.pre_release.clone(),
        )
    }
}

// Parse a Bazel version into a tuple of 3 integers
// Use "99", i.e. the highest possible version if a part is missing
// Parses pre-release versions, e.g. "pre.20240925.4" from "8.0.0-pre.20240925.4"
// Parses forked versions and stores the fork owner, e.g. "GitHubUser" from "GitHubUser/8.0.0"
fn parse_bazel_version(full_version_str: &str) -> Option<BazelVersion> {
    let (fork_owner, version_str) = match full_version_str.split_once('/') {
        Some((fork_owner, version_str)) => (Some(fork_owner.to_string()), version_str),
        None => (None, full_version_str),
    };
    let (version_str, pre_release) = match version_str.split_once('-') {
        Some((version_str, pre_release)) => (version_str, Some(pre_release.to_string())),
        None => (version_str, None),
    };
    let mut parts = version_str.split('.');
    let major = parts.next()?.parse::<i16>().ok()?;
    let minor_str = parts.next().unwrap_or("");
    if minor_str == "*" || minor_str == "+" {
        return Some(BazelVersion {
            major,
            minor: 99,
            patch: 0,
            fork_owner,
            pre_release,
        });
    }
    let minor = minor_str.parse::<i16>().unwrap_or(0);
    let patch_digits = parts
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    let patch = patch_digits.parse::<i16>().unwrap_or(99);
    Some(BazelVersion {
        major,
        minor,
        patch,
        fork_owner,
        pre_release,
    })
}

// Find the closest available Bazel version
pub fn find_closest_version(available_version_strs: &[String], version_hint_str: &str) -> String {
    let mut available_versions = available_version_strs
        .iter()
        .map(|s| (parse_bazel_version(s).unwrap().as_tuple(), s))
        .collect::<Vec<_>>();
    available_versions.sort();
    if let Some(version_hint) = parse_bazel_version(version_hint_str) {
        let match_idx = available_versions.partition_point(|e| e.0 <= version_hint.as_tuple());
        available_versions[match_idx.saturating_sub(1)].1.clone()
    } else {
        available_versions.last().unwrap().1.clone()
    }
}

// Use the Bazelisk logic to figure out the Bazel version
// Ref: https://github.com/bazelbuild/bazelisk/blob/1f9a1aca958cdb50b4adb84b15cdda55a600ed31/README.md?plain=1#L45-L47
pub fn determine_bazelisk_version(path: &Path) -> Option<String> {
    if let Ok(version_str) = env::var("USE_BAZEL_VERSION") {
        return Some(version_str.trim().to_string());
    }
    let workspace_root = get_workspace_path(path)?;
    if let Ok(bazeliskrc) = fs::read_to_string(workspace_root.join(".bazeliskrc")) {
        for line in bazeliskrc.split('\n') {
            if line.starts_with("USE_BAZEL_VERSION=") {
                let version_str = &line.split_once('=').unwrap().1;
                return Some(version_str.trim().to_string());
            }
        }
    }
    if let Ok(bazelversion) = fs::read_to_string(workspace_root.join(".bazelversion")) {
        return Some(bazelversion.trim().to_string());
    }

    None
}

pub static AVAILABLE_BAZEL_VERSIONS: Lazy<Vec<String>> =
    Lazy::new(|| load_packaged_bazel_flag_collection().all_bazel_versions);

pub fn auto_detect_bazel_version() -> Option<(String, Option<String>)> {
    if let Some(bazelisk_version) = determine_bazelisk_version(&env::current_dir().ok().unwrap()) {
        let bazel_version =
            find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), &bazelisk_version);
        if bazel_version == bazelisk_version {
            Some((bazel_version, None))
        } else {
            let message = format!(
                "Using flags from Bazel {} because flags for version {} are not available",
                bazel_version, bazelisk_version
            );
            Some((bazel_version, Some(message)))
        }
    } else {
        None
    }
}

#[test]
fn test_parse_bazel_version() {
    assert_eq!(
        parse_bazel_version("7.1.2"),
        Some(BazelVersion {
            major: 7,
            minor: 1,
            patch: 2,
            fork_owner: None,
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("7.*"),
        Some(BazelVersion {
            major: 7,
            minor: 99,
            patch: 0,
            fork_owner: None,
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("7.+"),
        Some(BazelVersion {
            major: 7,
            minor: 99,
            patch: 0,
            fork_owner: None,
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("7."),
        Some(BazelVersion {
            major: 7,
            minor: 0,
            patch: 99,
            fork_owner: None,
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("7"),
        Some(BazelVersion {
            major: 7,
            minor: 0,
            patch: 99,
            fork_owner: None,
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("8.1.1rc3"),
        Some(BazelVersion {
            major: 8,
            minor: 1,
            patch: 1,
            fork_owner: None,
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("9.0.0-pre.20210317.1"),
        Some(BazelVersion {
            major: 9,
            minor: 0,
            patch: 0,
            fork_owner: None,
            pre_release: Some("pre.20210317.1".to_string())
        })
    );
    assert_eq!(
        parse_bazel_version("GitHubUser/8.0.0"),
        Some(BazelVersion {
            major: 8,
            minor: 0,
            patch: 0,
            fork_owner: Some("GitHubUser".to_string()),
            pre_release: None
        })
    );
    assert_eq!(
        parse_bazel_version("GitHubUser/9.1.2-pre.20210317.1"),
        Some(BazelVersion {
            major: 9,
            minor: 1,
            patch: 2,
            fork_owner: Some("GitHubUser".to_string()),
            pre_release: Some("pre.20210317.1".to_string())
        })
    );
}

#[test]
fn test_find_closest_version() {
    let versions = [
        "7.0.0",
        "7.0.1",
        "7.0.2",
        "7.1.0",
        "7.1.1",
        "7.1.2",
        "7.2.0",
        "8.0.0",
        "8.0.1",
        "9.0.0-pre.20250121.1",
    ];
    let version_strings = versions.map(|s| s.to_string());
    // Versions with an exact match
    assert_eq!(find_closest_version(&version_strings, "7.1.1"), "7.1.1");
    assert_eq!(find_closest_version(&version_strings, "7.2.0"), "7.2.0");
    // An outdated version for which we no longer provide flags data
    assert_eq!(find_closest_version(&version_strings, "5.0.0"), "7.0.0");
    assert_eq!(find_closest_version(&version_strings, "5.1.1"), "7.0.0");
    // Release candidate versions
    assert_eq!(find_closest_version(&version_strings, "7.1.1rc2"), "7.1.1");
    assert_eq!(find_closest_version(&version_strings, "7.1.2rc2"), "7.1.2");
    assert_eq!(
        find_closest_version(&version_strings, "7.1.2-pre.123434"),
        "7.1.2"
    );
    // A newer patch version for which we don't have flags, yet
    assert_eq!(find_closest_version(&version_strings, "7.1.4"), "7.1.2");
    assert_eq!(find_closest_version(&version_strings, "7.2.3"), "7.2.0");
    assert_eq!(find_closest_version(&version_strings, "8.0.2"), "8.0.1");
    // A newer version, where we only have a pre-release version
    assert_eq!(
        find_closest_version(&version_strings, "9.1.2"),
        "9.0.0-pre.20250121.1"
    );
    // A partial version specification
    assert_eq!(find_closest_version(&version_strings, "7.*"), "7.2.0");
    assert_eq!(find_closest_version(&version_strings, "7.+"), "7.2.0");
    assert_eq!(find_closest_version(&version_strings, "7.1"), "7.1.2");
    assert_eq!(
        find_closest_version(&version_strings, "latest"),
        "9.0.0-pre.20250121.1"
    );
    assert_eq!(
        find_closest_version(&version_strings, "latest-1"),
        "9.0.0-pre.20250121.1"
    );
}
