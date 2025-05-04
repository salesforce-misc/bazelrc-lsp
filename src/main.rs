use std::io::Read;
use std::ops::Deref;
use std::path::Path;
use std::{env, fs, io, process};

use bazelrc_lsp::bazel_flags::{
    load_bazel_flags_from_command, load_packaged_bazel_flags, BazelFlags,
};
use bazelrc_lsp::bazel_version::{
    determine_bazelisk_version, find_closest_version, AVAILABLE_BAZEL_VERSIONS,
};
use bazelrc_lsp::diagnostic::diagnostics_from_string;
use bazelrc_lsp::formatting::{pretty_print, FormatLineFlow};
use bazelrc_lsp::language_server::{Backend, Settings};
use bazelrc_lsp::lsp_utils::LspPositionEncoding;
use clap::{CommandFactory, Parser, Subcommand};
use tower_lsp::{LspService, Server};
use walkdir::WalkDir;

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
            FormatLineFlowCli(FormatLineFlow::LineContinuations),
            FormatLineFlowCli(FormatLineFlow::SeparateLines),
            FormatLineFlowCli(FormatLineFlow::SingleLine),
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self.0 {
            FormatLineFlow::Keep => Some(clap::builder::PossibleValue::new("keep")),
            FormatLineFlow::LineContinuations => {
                Some(clap::builder::PossibleValue::new("line-continuations"))
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
    /// Check your bazelrc files for mistakes
    Lint(LintArgs),
    /// List supported Bazel versions
    #[clap(hide = true)]
    BazelVersions {},
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
                position_encoding: LspPositionEncoding::UTF16.into(),
                settings: Settings {
                    format_lines: cli.format_lines.0,
                }
                .into(),
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
        Commands::Lint(args) => {
            handle_lint_cmd(&args, &bazel_flags);
        }
        Commands::BazelVersions {} => {
            println!(
                "{}",
                serde_json::to_string(AVAILABLE_BAZEL_VERSIONS.deref()).unwrap()
            );
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

fn for_each_input_file<CB>(files: &[String], handle_file: CB) -> bool
where
    CB: Fn(String, Option<&Path>) -> bool,
{
    let mut had_errors = false;

    if files.is_empty() {
        // Read complete stdin
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .expect("Failed to read from stdin");
        had_errors |= handle_file(input, None);
    } else {
        for path_str in files {
            let path = std::path::Path::new(path_str);
            if path.is_dir() {
                let walker = WalkDir::new(path).into_iter().filter_entry(|e| {
                    let s = e.file_name().to_string_lossy();
                    // We want to skip all hidden sub-directories, but still visit `.bazelrc` files
                    // and also work if the user called `bazelrc-lsp format .`
                    !s.starts_with('.') || s == "." || s == ".." || s == ".bazelrc"
                });
                for entry in walker {
                    match entry {
                        Ok(entry) => {
                            let subpath = entry.into_path();
                            let has_bazelrc_suffix =
                                subpath.to_string_lossy().ends_with(".bazelrc");
                            if has_bazelrc_suffix && subpath.is_file() {
                                let input =
                                    fs::read_to_string(&subpath).expect("Failed to read file");
                                had_errors |= handle_file(input, Some(subpath.as_path()));
                            }
                        }
                        Err(err) => {
                            eprintln!(
                                "Failed to enumerate files in `{}`: {}",
                                path_str,
                                err.io_error().unwrap()
                            );
                            had_errors = true;
                        }
                    }
                }
            } else {
                let input = fs::read_to_string(path).expect("Failed to read file");
                had_errors |= handle_file(input, Some(path));
            }
        }
    }

    had_errors
}

#[derive(Parser)]
struct LintArgs {
    /// File(s) to format
    files: Vec<String>,
    /// Suppress output and only indicate errors through the exit code
    #[arg(long, group = "fmt-action")]
    quiet: bool,
}

fn handle_lint_cmd(args: &LintArgs, bazel_flags: &BazelFlags) {
    let had_errors = for_each_input_file(&args.files, |input: String, path: Option<&Path>| {
        let diagnostics =
            diagnostics_from_string(&input, bazel_flags, path, LspPositionEncoding::UTF32);
        if !args.quiet {
            for d in &diagnostics {
                // TODO: improve printing, either using ariadne or codespan-reporting
                println!(
                    "{}: {}",
                    path.and_then(Path::to_str).unwrap_or("<stdin>"),
                    d.message
                );
            }
        }
        !diagnostics.is_empty()
    });
    if had_errors {
        process::exit(1);
    }
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

fn handle_format_cmd(args: &FormatArgs, bazel_flags: &BazelFlags, line_flow: FormatLineFlow) {
    if args.inplace && args.files.is_empty() {
        let mut cmd = Cli::command();
        cmd.error(
            clap::error::ErrorKind::ArgumentConflict,
            "If the `-i` flag is specified, input file(s) must be specified as part of the command line invocation",
        ).exit();
    }

    let had_errors = for_each_input_file(&args.files, |input: String, path: Option<&Path>| {
        let result = pretty_print(&input, bazel_flags, line_flow);
        match result {
            Ok(formatted) => {
                if args.check {
                    let input_name = path
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or("<stdin>".to_string());
                    if formatted != input {
                        println!(
                            "{} is NOT correctly formatted and needs reformatting",
                            input_name
                        );
                        return true;
                    } else {
                        println!("{} is already correctly formatted", input_name);
                    }
                } else if args.inplace {
                    fs::write(path.unwrap(), formatted).expect("Failed to write file");
                } else {
                    if let Some(p) = path {
                        println!("--- {} ---", p.to_string_lossy());
                    }
                    print!("{}", formatted);
                }
            }
            Err(errors) => {
                for e in errors {
                    eprintln!("{}", e);
                }
                return true;
            }
        };
        false
    });
    if had_errors {
        process::exit(1);
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}
