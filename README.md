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

* Syntax highlighting / semantic tokens
* Diagnose invalid command line args
  * unknown args
  * repeated args
* Show documentation of flags on hover
* Autocomplete
  * flags
  * command names
  * config names
* Format / pretty print
* Rename functionality for config names
* `import` support
