pub mod bazel_flags;
pub mod diagnostic;
pub mod lsp_utils;
pub mod parser;

pub mod bazel_flags_proto {
    include!(concat!(env!("OUT_DIR"), "/protobuf/bazel_flags.rs"));
}
