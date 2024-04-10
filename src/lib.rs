pub mod bazel_flags;
pub mod completion;
pub mod diagnostic;
pub mod formatting;
pub mod line_index;
pub mod lsp_utils;
pub mod parser;
pub mod semantic_token;
pub mod tokenizer;

pub mod bazel_flags_proto {
    include!(concat!(env!("OUT_DIR"), "/protobuf/bazel_flags.rs"));
}
