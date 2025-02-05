use bazelrc_lsp::bazel_flags::{
    load_bazel_flags_from_command, load_packaged_bazel_flags, BazelFlags,
};
use bazelrc_lsp::bazel_version::{
    auto_detect_bazel_version, find_closest_version, AVAILABLE_BAZEL_VERSIONS,
};
use bazelrc_lsp::language_server::Backend;
use std::env;
use tower_lsp::{LspService, Server};

fn load_bazel_flags() -> (BazelFlags, Option<String>) {
    if let Ok(bazel_command) = env::var("BAZELRC_LSP_RUN_BAZEL_PATH") {
        match load_bazel_flags_from_command(&bazel_command) {
            Ok(flags) => (flags, None),
            Err(msg) => {
                let bazel_version =
                    find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), "latest");
                let message =
                    format!("Using flags from Bazel {bazel_version} because running `{bazel_command}` failed:\n{}\n", msg);
                (load_packaged_bazel_flags(&bazel_version), Some(message))
            }
        }
    } else if let Some(auto_detected) = auto_detect_bazel_version() {
        return (load_packaged_bazel_flags(&auto_detected.0), auto_detected.1);
    } else {
        let bazel_version = find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), "latest");
        let message = format!(
            "Using flags from Bazel {bazel_version} because auto-detecting the Bazel version failed"        );
        (load_packaged_bazel_flags(&bazel_version), Some(message))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (bazel_flags, version_message) = load_bazel_flags();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: Default::default(),
        bazel_flags,
        startup_warning: version_message,
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
