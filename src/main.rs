use std::env;

use bazelrc_lsp::bazel_flags::{
    load_bazel_flags_from_command, load_packaged_bazel_flags, BazelFlags,
};
use bazelrc_lsp::bazel_version::{
    determine_bazelisk_version, find_closest_version, AVAILABLE_BAZEL_VERSIONS,
};
use bazelrc_lsp::language_server::Backend;
use clap::Parser;
use tower_lsp::{LspService, Server};

#[derive(Parser)]
#[command(version)]
#[command(about = "Code Intelligence for bazelrc config files")]
struct Cli {
    /// The Bazel version
    #[arg(long, value_name = "VERSION")]
    bazel_version: Option<String>,
    /// Path to a Bazel version
    #[arg(long, value_name = "PATH")]
    bazel_path: Option<String>,
}

#[tokio::main]
async fn main() {
    let mut cli = Cli::parse();
    if cli.bazel_version.is_some() && cli.bazel_path.is_some() {
        eprintln!("Either `--bazel-version` or `--bazel-path` can be set, but not both.");
        std::process::exit(1);
    }
    if cli.bazel_version.is_none() && cli.bazel_path.is_none() {
        // The bazel path can also provided as an environment variable
        cli.bazel_path = env::var("BAZELRC_LSP_RUN_BAZEL_PATH").ok();
    }

    let (bazel_flags, version_message) = load_bazel_flags(&cli);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: Default::default(),
        bazel_flags,
        startup_warning: version_message,
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn load_bazel_flags(cli: &Cli) -> (BazelFlags, Option<String>) {
    if let Some(bazel_command) = &cli.bazel_path {
        match load_bazel_flags_from_command(bazel_command) {
            Ok(flags) => (flags, None),
            Err(msg) => {
                let bazel_version =
                    find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), "latest").0;
                let message =
                    format!("Using flags from Bazel {bazel_version} because running `{bazel_command}` failed:\n{}\n", msg);
                (load_packaged_bazel_flags(&bazel_version), Some(message))
            }
        }
    } else if let Some(cli_version) = &cli.bazel_version {
        let (bazel_version, msg) =
            find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), cli_version);
        (load_packaged_bazel_flags(&bazel_version), msg)
    } else if let Some(auto_detected) = determine_bazelisk_version(&env::current_dir().unwrap()) {
        let (bazel_version, msg) =
            find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), &auto_detected);
        (load_packaged_bazel_flags(&bazel_version), msg)
    } else {
        let bazel_version = find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), "latest").0;
        let message = format!(
            "Using flags from Bazel {bazel_version} because auto-detecting the Bazel version failed");
        (load_packaged_bazel_flags(&bazel_version), Some(message))
    }
}
