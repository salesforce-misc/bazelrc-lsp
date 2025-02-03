use base64::prelude::*;
use prost::{bytes::Bytes, Message};
use std::{collections::HashMap, env, fs, io::Result, path::Path, process::Command};

include!("src/bazel_flags_proto.rs");

fn dump_flags(cache_dir: &Path, version: &str) -> Vec<u8> {
    let cache_path = cache_dir.join(format!("flags-dumps/{version}.data"));
    if cache_path.exists() {
        fs::read(cache_path).unwrap()
    } else {
        let mut bazelisk_cmd = if cfg!(windows) {
            // In Github Actions, bazelisk is available via powershell on Windows
            let mut cmd = Command::new("powershell.exe");
            cmd.arg("-File");
            cmd.arg("C:\\npm\\prefix\\bazelisk.ps1");
            cmd
        } else {
            Command::new("bazelisk")
        };
        let result = bazelisk_cmd
            .env("USE_BAZEL_VERSION", version)
            .arg("help")
            .arg("flags-as-proto")
            .output()
            .unwrap_or_else(|e| panic!("Failed to spawn Bazelisk for version {version}, {e}"));
        if !result.status.success() {
            panic!(
                "Failed to get flags for Bazel version {version}:\n===stdout===\n{stdout}\n===stderr===\n{stderr}",
                stdout = String::from_utf8_lossy(&result.stdout),
                stderr = String::from_utf8_lossy(&result.stderr)
            );
        }
        let flags_binary = BASE64_STANDARD
            .decode(result.stdout)
            .expect("Failed to decode Bazelisk output as base64");
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| {
                panic!(
                    "Failed to create directory at {} for flags, {e}",
                    parent.display()
                )
            });
        }
        fs::write(cache_path.clone(), &flags_binary).unwrap_or_else(|e| {
            panic!(
                "Failed to write flags to disk at {}, {e}",
                cache_path.display()
            )
        });
        flags_binary
    }
}

fn merge_flags_into(
    new_flags: Vec<FlagInfo>,
    flags: &mut HashMap<String, Vec<FlagInfo>>,
    bazel_version: &str,
) {
    new_flags.into_iter().for_each(|mut new_flag| {
        if let Some(existing_flags) = flags.get_mut(&new_flag.name) {
            let existing_flag_opt = existing_flags.iter_mut().find(|existing_flag| {
                existing_flag.has_negative_flag == new_flag.has_negative_flag
                    && existing_flag.documentation == new_flag.documentation
                    && existing_flag.commands == new_flag.commands
                    && existing_flag.abbreviation == new_flag.abbreviation
                    && existing_flag.allows_multiple == new_flag.allows_multiple
                    && existing_flag.effect_tags == new_flag.effect_tags
                    && existing_flag.metadata_tags == new_flag.metadata_tags
                    && existing_flag.documentation_category == new_flag.documentation_category
                    && existing_flag.requires_value == new_flag.requires_value
            });
            if let Some(existing_flag) = existing_flag_opt {
                existing_flag.bazel_versions.push(bazel_version.to_string());
            } else {
                new_flag.bazel_versions.push(bazel_version.to_string());
                existing_flags.push(new_flag);
            }
        } else {
            new_flag.bazel_versions.push(bazel_version.to_string());
            flags.insert(new_flag.name.clone(), vec![new_flag]);
        }
    });
}

fn main() -> Result<()> {
    let versions = [
        "7.0.0",
        "7.0.1",
        "7.0.2",
        "7.1.0",
        "7.1.1",
        "7.1.2",
        "7.2.0",
        "7.2.1",
        "7.3.0",
        "7.3.1",
        "7.3.2",
        "7.4.0",
        "7.4.1",
        "7.5.0",
        "8.0.0",
        "8.0.1",
        "9.0.0-pre.20250121.1",
    ];

    let cache_dir = env::current_dir().unwrap().join("bazel-flags-cache");
    if !cache_dir.exists() {
        fs::create_dir(&cache_dir).expect("Failed to create cached directory");
    }

    let mut flags_by_name = HashMap::<String, Vec<FlagInfo>>::new();
    for version in versions {
        let flags_proto: Vec<u8> = dump_flags(&cache_dir, version);
        let flags = FlagCollection::decode(Bytes::from(flags_proto))
            .expect("Failed to decode protobuf flags");
        merge_flags_into(flags.flag_infos, &mut flags_by_name, version);
    }

    // Hack to workaround https://github.com/salesforce-misc/bazelrc-lsp/issues/2
    // Bazel used to have two `--watchfs` flags: A startup-flag and a build flag.
    // The build flag is mising from the flag-dumps of older Bazel versions, and only
    // the startup flag was included, which is marked as deprecated. Newer Bazel versions
    // only report the non-deprecated build flag.
    //
    // We "back-port" this fix to earlier Bazel versions by patching the flags here.
    let watchfs_flags = flags_by_name.remove("watchfs").unwrap();
    let (mut deprecated_watchfs, mut non_deprecated_watchfs): (Vec<_>, Vec<_>) = watchfs_flags
        .into_iter()
        .partition(|f| f.metadata_tags.contains(&"DEPRECATED".to_string()));
    for flag in &mut deprecated_watchfs {
        non_deprecated_watchfs[0]
            .bazel_versions
            .append(&mut flag.bazel_versions);
    }
    flags_by_name.insert("watchfs".to_string(), non_deprecated_watchfs);

    // Write the combined flags into a file
    let flag_list = flags_by_name
        .into_iter()
        .flat_map(|e| e.1)
        .collect::<Vec<_>>();
    let combined_flag_collection = FlagCollection {
        flag_infos: flag_list,
        all_bazel_versions: Vec::from(versions.map(|f| f.to_string())),
    };
    let combined_proto = combined_flag_collection.encode_to_vec();
    let compressed = lz4_flex::compress_prepend_size(&combined_proto);
    let out_dir_env = env::var_os("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir_env);
    let result_path: std::path::PathBuf = out_dir.join("bazel-flags-combined.data.lz4");
    fs::write(result_path, compressed).expect("Failed to write combined flags to disk");

    Ok(())
}
