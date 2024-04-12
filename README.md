# bazelrc language server

Code intelligence for `.bazelrc` config files.

## Development

The source code for this extension lives at https://github.com/vogelsgesang/bazelrc-lsp.
Contributions are welcome. Feel free to just open a pull request.

### Building from source

1. `cd vscode-extension`
2. `pnpm i`
3. `pnpm package`
4. Install the "hyper-ir-lsp-*.vsix" in VS Code

### Backlog

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
  * flag names:
    * ✔ basic auto-complete
    * ✔ insert `--` prefix for options
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
    * "line reflowing" support (one flag per line; one flag per command; ...)
    * compact multiple consecutive empty lines
    * break up multiline comments
    * more aggressive reformatting of comments / smarter detection of ASCII art
  * ✔ LSP integration
    * ✔ whole document formatting
    * ✔ range formatting
  * expose through command line
* ✔ link file names for `import` & `try-import`
* Rename functionality for config names
* Bazel-side changes:
  * expose default value, value description and old names and deprecation messages
* References:
  * other usages of config name
  * find other usages of same flag
* Support flags with same name on different commands. E.g., `watchfs` which is deprecated as a startup action, but still is supported as a flag to the `build` command
* Bazel version support
  * pack multiple flag versions & allow selection via flag
  * run `bazel help flags-as-proto` at runtime
