import {
  workspace,
  type ExtensionContext
} from 'vscode';

import {
  type Executable,
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions
} from 'vscode-languageclient/node';

async function startLsp (context: ExtensionContext) {
  const command = process.env.SERVER_PATH ?? context.asAbsolutePath('bazelrc-lsp');

  const config = workspace.getConfiguration('bazelrc');
  const lineFlow = config.get<string>('format.lineFlow') ?? 'keep';
  const bazelVersion = config.get<string>('bazelVersion') ?? 'auto-detect';
  const bazelVersionArgs =
    bazelVersion !== 'auto-detect' ? ['--bazel-version', bazelVersion] : [];

  const run: Executable = {
    command,
    args: bazelVersionArgs.concat(['--format-lines', lineFlow, 'lsp']),
    options: {
      env: {
        ...process.env,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        RUST_LOG: 'debug',
        // eslint-disable-next-line @typescript-eslint/naming-convention
        RUST_BACKTRACE: '1'
      }
    }
  };
  // If the extension is launched in debug mode then the debug server options are used
  // Otherwise the run options are used
  const serverOptions: ServerOptions = {
    run,
    debug: run
  };
  // Options to control the language client
  const clientOptions: LanguageClientOptions = {
    // Register the server for bazelrc documents
    documentSelector: [{ language: 'bazelrc' }]
  };

  // Create the language client and start the client.
  const client = new LanguageClient('bazelrc-lsp', 'Bazelrc Language Server', serverOptions, clientOptions);
  await client.start();
  return client;
}

let client: LanguageClient | null = null;

export async function activate (context: ExtensionContext) {
  client = await startLsp(context);

  context.subscriptions.push(workspace.onDidChangeConfiguration(async (e) => {
    if (e.affectsConfiguration('bazelrc.bazelVersion') ||
       e.affectsConfiguration('bazelrc.format.lineFlow')) {
      await client?.stop();
      client = await startLsp(context);
    }
  }));
}

export function deactivate (): Thenable<void> | undefined {
  if (client === null) {
    return undefined;
  }
  return client.stop();
}
