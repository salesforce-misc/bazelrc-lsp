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
  * highlight deprecated options
* Diagnose invalid command line args
  * unknown args
  * repeated args
* ✔ Hover
  * ✔ Show documentation of flags on hover
  * ✔ Show documentation for commands on hover
* Autocomplete
  * ✔ basic auto-complete
  * context-aware auto-complete
    * ✔ flags
    * ✔ command names
    * config names based on other config names
    * values based on previously observed values
  * auto-complete based on category / tags
* Format / pretty print
* Rename functionality for config names
* References: Find other usages of same flag
* `import` support
