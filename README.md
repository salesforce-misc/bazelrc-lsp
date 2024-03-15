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
* Diagnose
  * unknown args
  * repeated args
  * configs on `startup`, `import`, `try-import`; including fix
  * empty config name; including fix
  * config name which doesn't match `[a-z_\-]+`
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
  * insert trailing space where appropriate
  * insert `--` prefix for options
  * auto-complete based on category / tags
* Format / pretty print
* `import` support
  * link file names
  * diagnose if file is not found
* Rename functionality for config names
* References:
  * other usages of config name
  * Find other usages of same flag
