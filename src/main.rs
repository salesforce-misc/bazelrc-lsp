use std::io::Read;
use std::{env, fs, io, process};

use bazelrc_lsp::bazel_flags::{
    load_bazel_flags_from_command, load_packaged_bazel_flags, BazelFlags,
};
use bazelrc_lsp::bazel_version::{
    determine_bazelisk_version, find_closest_version, AVAILABLE_BAZEL_VERSIONS,
};
use bazelrc_lsp::formatting::{pretty_print, FormatLineFlow};
use bazelrc_lsp::language_server::Backend;
use clap::{Parser, Subcommand};
use tower_lsp::{LspService, Server};

#[derive(Parser)]
#[command(version)]
#[command(about = "Code Intelligence for bazelrc config files")]
struct Cli {
    /// The Bazel version
    #[arg(long, value_name = "VERSION", group = "bazel-version")]
    bazel_version: Option<String>,
    /// Path to a Bazel version
    #[arg(long, value_name = "PATH", group = "bazel-version")]
    bazel_path: Option<String>,
    /// Should lines be combined / split when formatting bazelrc files?
    #[arg(long, default_value = "keep")]
    format_lines: FormatLineFlowCli,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy)]
struct FormatLineFlowCli(FormatLineFlow);
impl clap::ValueEnum for FormatLineFlowCli {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            FormatLineFlowCli(FormatLineFlow::Keep),
            FormatLineFlowCli(FormatLineFlow::LineContinuation),
            FormatLineFlowCli(FormatLineFlow::SeparateLines),
            FormatLineFlowCli(FormatLineFlow::SingleLine),
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self.0 {
            FormatLineFlow::Keep => Some(clap::builder::PossibleValue::new("keep")),
            FormatLineFlow::LineContinuation => {
                Some(clap::builder::PossibleValue::new("line-continuation"))
            }
            FormatLineFlow::SeparateLines => {
                Some(clap::builder::PossibleValue::new("separate-lines"))
            }
            FormatLineFlow::SingleLine => Some(clap::builder::PossibleValue::new("single-line")),
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Spawns the language server
    Lsp {},
    /// Format a bazelrc file
    ///
    /// If no arguments are specified, format the bazelrc contents
    /// from stdin and write the result to stdout.
    /// If <file>s are given, reformat the files. If -i is specified,
    /// the files are edited in-place. Otherwise, the result is written to the stdout.
    Format(FormatArgs),
}

#[derive(Parser)]
struct FormatArgs {
    /// File(s) to format
    files: Vec<String>,
    /// Inplace edit <file>s
    #[arg(short = 'i', long, group = "fmt-action")]
    inplace: bool,
    /// Only check if the given file(s) are formatted correctly
    #[arg(long, group = "fmt-action")]
    check: bool,
}

#[tokio::main]
async fn main() {
    let mut cli = Cli::parse();
    if cli.bazel_version.is_none() && cli.bazel_path.is_none() {
        // The bazel path can also provided as an environment variable
        cli.bazel_path = env::var("BAZELRC_LSP_RUN_BAZEL_PATH").ok();
    }
    // For backwards compatibility: If no command is specified, assume we should
    // launch the language server.
    cli.command = Some(cli.command.unwrap_or(Commands::Lsp {}));

    let (bazel_flags, version_message) = load_bazel_flags(&cli);

    match cli.command.unwrap() {
        Commands::Lsp {} => {
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();

            let (service, socket) = LspService::new(|client| Backend {
                client,
                document_map: Default::default(),
                bazel_flags,
                format_line_flow: cli.format_lines.0,
                startup_warning: version_message,
            });
            Server::new(stdin, stdout, socket).serve(service).await;
        }
        Commands::Format(args) => {
            if let Some(msg) = &version_message {
                eprintln!("{}", msg);
            }
            handle_format_cmd(&args, &bazel_flags, cli.format_lines.0);
        }
    };
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

fn handle_format_cmd(args: &FormatArgs, bazel_flags: &BazelFlags, line_flow: FormatLineFlow) {
    let mut had_errors = false;

    let mut do_format = |input: String, file_name: Option<&str>| {
        let result = pretty_print(&input, bazel_flags, line_flow);
        match result {
            Ok(formatted) => {
                if args.check {
                    let input_name = file_name
                        .map(|name| format!("`{}`", name))
                        .unwrap_or("<stdin>".to_string());
                    if formatted != input {
                        println!(
                            "{} is NOT correctly formatted and needs reformatting",
                            input_name
                        );
                        had_errors = true;
                    } else {
                        println!("{} is already correctly formatted", input_name);
                    }
                } else if args.inplace {
                    if let Some(file_name) = file_name {
                        fs::write(file_name, formatted).expect("Failed to write file");
                    } else {
                        eprintln!(
                            "The `-i` flag does not make sense when not specifying any files"
                        );
                        process::exit(1);
                    }
                } else {
                    if let Some(file_name) = file_name {
                        println!("--- {} ---", file_name);
                    }
                    print!("{}", formatted);
                }
            }
            Err(errors) => {
                for e in errors {
                    eprintln!("{}", e);
                }
                had_errors = true;
            }
        };
    };

    if args.files.is_empty() {
        // Read complete stdin
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .expect("Failed to read from stdin");
        do_format(input, None);
    } else {
        for file_name in &args.files {
            let input = fs::read_to_string(file_name).expect("Failed to read file");
            do_format(input, Some(file_name));
        }
    }

    if had_errors {
        process::exit(1);
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
