# bazelrc language server

Code intelligence for `.bazelrc` config files.

## Installation

The language server from this repository can be used in a wide range editors (neovim, emacs, IntelliJ, ...).

For **Visual Studio Code**, we offer a pre-packaged Visual Studio Code plugin:

1. Download the correct `*.vsix` package for your operating system from the [latest release](https://github.com/salesforce-misc/bazelrc-lsp/releases/)
2. Inside Visual Studio, press `Cmd` + `Shift` + `P` to open the command picker
3. Choose the "Extension: Install from VSIX..." command
4. In the file picker, choose the downloaded `.vsix` file

I will leave it as an exercise to the reader to figure out how exactly
to configure the language server for other editors. (Pull requests welcome).

## Bazel version

`bazelrc-lsp` comes packaged with the list of supported Bazel flags for a large range of Bazel versions.
The exact Bazel version is selected the same way as in Bazelisk:
If a `USE_BAZEL_VERSION` environment variable is present, then this environment variable is authorative.
Otherwise, the Bazel version will be determined from the `.bazeliskrc` or the `.bazelversion` file.

Alternatively, the language server can also invoke Bazel to request an up-to-date list of supported flags.
To do so, point the environment variable `BAZELRC_LSP_RUN_BAZEL_PATH` to your Bazel binary or use the
`--bazel-path` command line parameter.

## Current State & Roadmap

The extension is complete enough for my personal needs and hopefully useful to you, too.

Long-term, I am considering to integrate this functionality into the official [VSCode Bazel extension](https://github.com/bazelbuild/vscode-bazel). This is also why this extension is not published to the VS Code Marketplace as a standalone extension.

However, currently this extension still has a couple rough edges. Before integrating this language server with the VSCode Bazel extension, and thereby exposing it to a larger user base, those sharp edges first need to be smoothed.

Pull Requests are welcome! Further down in this README you can find a backlog of various ideas.
In case you want to discuss any of those topics (or a topic of your own), please feel free to reach out via a Github issue.

## Development

The source code for this extension lives at https://github.com/salesforce-misc/bazelrc-lsp.
Contributions are welcome. Feel free to just open a pull request.

### Building from source

1. `cd vscode-extension`
2. `pnpm i`
3. `pnpm package`
4. Install the "hyper-ir-lsp-*.vsix" in VS Code

### Backlog

* ✔ Bazel version support
  * ✔ load flags from Bazel's flag dump
  * ✔ pack multiple flag versions & auto-detect version
  * ✔ run `bazel help flags-as-proto` at runtime based on environment variable
  * ✔ expose settings in VS-Code extension
* ✔ Syntax highlighting / semantic tokens
* Diagnose
  * ✔ unknown flags
  * ✔ allow custom setting flags (`--//my/package:setting` and `--no//my/package:setting`)
  * repeated flags
  * abbreviated flag names; prefer non-abbreviated flags
  * ✔ diagnose deprecated flags
  * ✔ diagnose missing `import`ed files
  * ✔ configs on `startup`, `import`, `try-import`
  * ✔ empty config name
  * ✔ config name which doesn't match `[a-z_\-]+` (or similar)
  * offer fix-it:
    * to remove repeated flags
    * to replace abbreviated flags by non-abbreviated flags
    * to remove deprecated no-op flags
    * to fix config-name-related issues
* ✔ Hover
  * ✔ Show documentation of flags on hover
  * ✔ Correctly escape `<>` in Markdown (e.g. problematic in the documentation for `--config`)
  * Link to flag documentation in hovers
  * ✔ Show documentation for commands on hover
* Autocomplete
  * ✔ auto complete command names
  * ✔ flag names
  * flag values:
    * based on available setting values (needs Bazel-side changes)
    * based on previously observed values
  * config names
    * based on config names used elsewhere in the file / project
  * file names for `import` / `try-import`
* Format / pretty print
  * improved formatting behavior
    * ✔ basic formatting support
    * ✔ always quote arguments to `import` / `try-import`
    * ✔ combine `--flag value` into `--flag=value`
    * ✔ "line reflowing" support (all on single line; one flag per line with `\` line continuations; one flag per command; ...)
    * ✔ compact multiple consecutive empty lines
    * break up multiline continuations in comments
    * more aggressive reformatting of comments / smarter detection of Markdown, ASCII art etc.
  * ✔ LSP integration
    * ✔ whole document formatting
    * ✔ range formatting
  * expose formatting through command line to enable integration into CI systems
    * ✔ stdin processing
    * ✔ inplace updates for files
    * ✔ `--check`-mode
    * support enumerating all bazelrc files from a folder
  * write documentation, including explanation of different styles
* ✔ link file names for `import` & `try-import`
* Rename functionality for config names
* Bazel-side changes:
  * expose default value, value description and old names and deprecation messages
* Go to Reference:
  * other usages of config name
  * find other usages of same flag