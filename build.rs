use std::io::Result;

fn main() -> Result<()> {
    // Workaround for hombrew on Mac:
    // std::env::set_var("PROTOC", "/usr/local/bin/protoc");

    let mut protobuf_out = std::path::PathBuf::new();
    protobuf_out.push(&std::env::var("OUT_DIR").unwrap());
    protobuf_out.push("protobuf");
    std::fs::create_dir(&protobuf_out).ok();

    prost_build::Config::new()
        .out_dir(&protobuf_out)
        .compile_protos(&["proto/bazel_flags.proto"], &["proto/"])?;

    Ok(())
}
