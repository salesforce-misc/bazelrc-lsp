// Based on https://github.com/bazelbuild/bazel/blob/master/src/main/protobuf/bazel_flags.proto
// Originaly generated via prost-build.
// This file contains additional modifications to store
// supported Bazel version ranges.

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FlagInfo {
    /// Name of the flag, without leading dashes.
    #[prost(string, required, tag = "1")]
    pub name: ::prost::alloc::string::String,
    /// True if --noname exists, too.
    #[prost(bool, optional, tag = "2", default = "false")]
    pub has_negative_flag: ::core::option::Option<bool>,
    /// Help text of the flag.
    #[prost(string, optional, tag = "3")]
    pub documentation: ::core::option::Option<::prost::alloc::string::String>,
    /// List of supported Bazel commands, e.g. \['build', 'test'\]
    #[prost(string, repeated, tag = "4")]
    pub commands: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// Flag name abbreviation, without leading dash.
    #[prost(string, optional, tag = "5")]
    pub abbreviation: ::core::option::Option<::prost::alloc::string::String>,
    /// True if a flag is allowed to occur multiple times in a single arg list.
    #[prost(bool, optional, tag = "6", default = "false")]
    pub allows_multiple: ::core::option::Option<bool>,
    /// The effect tags associated with the flag
    #[prost(string, repeated, tag = "7")]
    pub effect_tags: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// The metadata tags associated with the flag
    #[prost(string, repeated, tag = "8")]
    pub metadata_tags: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// The documentation category assigned to this flag
    #[prost(string, optional, tag = "9")]
    pub documentation_category: ::core::option::Option<::prost::alloc::string::String>,
    /// Whether the flag requires a value.
    /// If false, value-less invocations are acceptable, e.g. --subcommands,
    /// but if true a value must be present for all instantiations of the flag,
    /// e.g. --jobs=100.
    #[prost(bool, optional, tag = "10")]
    pub requires_value: ::core::option::Option<bool>,
    // The old, deprecated name for this option, without leading dashes.
    // TODO: Fix the tag number after the upstream Bazel change got merged.
    // See https://github.com/bazelbuild/bazel/pull/25169
    #[prost(string, optional, tag = "99998")]
    pub old_name: Option<::prost::alloc::string::String>,
    // The deprecation warning for this option, if one is present.
    // TODO: Fix the tag number after the upstream Bazel change got merged.
    // See https://github.com/bazelbuild/bazel/pull/25169
    #[prost(string, optional, tag = "99999")]
    pub deprecation_warning: Option<::prost::alloc::string::String>,

    /// EXTENSION: List of Bazel versions this flag applies to
    #[prost(string, repeated, tag = "999")]
    pub bazel_versions: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FlagCollection {
    #[prost(message, repeated, tag = "1")]
    pub flag_infos: ::prost::alloc::vec::Vec<FlagInfo>,
    /// EXTENSION: List of Bazel versions indexed in this collection
    #[prost(string, repeated, tag = "999")]
    pub all_bazel_versions: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
