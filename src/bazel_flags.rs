use prost::Message;
use std::{collections::HashMap, io::Cursor};

use crate::bazel_flags_proto::{FlagCollection, FlagInfo};

#[derive(Debug)]
pub struct BazelFlags {
    pub flags: Vec<FlagInfo>,
    pub flags_by_commands: HashMap<String, Vec<usize>>,
    pub flags_by_name: HashMap<String, usize>,
    pub flags_by_abbreviation: HashMap<String, usize>,
}

impl BazelFlags {
    pub fn from_flags(flags: Vec<FlagInfo>) -> BazelFlags {
        let mut flags_by_commands = HashMap::<String, Vec<usize>>::new();
        let mut flags_by_name = HashMap::<String, usize>::new();
        let mut flags_by_abbreviation = HashMap::<String, usize>::new();
        for (i, f) in (&flags).iter().enumerate() {
            for c in &f.commands {
                let list = flags_by_commands
                    .entry(c.clone())
                    .or_insert_with(|| Default::default());
                list.push(i);
            }
            flags_by_name.insert(f.name.clone(), i);
            if let Some(abbreviation) = &f.abbreviation {
                flags_by_abbreviation.insert(abbreviation.clone(), i);
            }
        }
        return BazelFlags {
            flags: flags,
            flags_by_commands,
            flags_by_name,
            flags_by_abbreviation,
        };
    }
}

pub fn load_bazel_flags() -> BazelFlags {
    let proto_bytes = include_bytes!("../flag-dumps/7.1.0.data");
    let flags = FlagCollection::decode(&mut Cursor::new(proto_bytes))
        .unwrap()
        .flag_infos;
    return BazelFlags::from_flags(flags);
}

#[test]
fn test_flags() {
    let flags = load_bazel_flags();
    let mut commands = flags
        .flags_by_commands
        .keys()
        .map(|s| s.clone())
        .collect::<Vec<_>>();
    commands.sort();
    assert_eq!(
        commands,
        vec!(
            "analyze-profile",
            "aquery",
            "build",
            "canonicalize-flags",
            "clean",
            "config",
            "coverage",
            "cquery",
            "dump",
            "fetch",
            "help",
            "info",
            "license",
            "mobile-install",
            "mod",
            "print_action",
            "query",
            "run",
            "shutdown",
            "startup",
            "sync",
            "test",
            "vendor",
            "version"
        )
    );

    // Command info
    let preemptible_info = flags.flags.iter().find(|f| f.name == "preemptible");
    assert_eq!(
        preemptible_info
            .unwrap()
            .commands
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>(),
        vec!("startup")
    );
}
