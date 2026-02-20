import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Trace,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  await startClient(context);

  const restart = vscode.commands.registerCommand('aic.restartLanguageServer', async () => {
    await stopClient();
    await startClient(context);
    void vscode.window.showInformationMessage('AICore language server restarted.');
  });

  context.subscriptions.push(restart);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  if (client) {
    return;
  }

  const cfg = vscode.workspace.getConfiguration('aic');
  const command = cfg.get<string>('server.path', 'aic');
  const args = cfg.get<string[]>('server.args', ['lsp']);
  const trace = cfg.get<string>('trace.server', 'off');

  const serverOptions: ServerOptions = {
    command,
    args,
    options: {
      env: process.env,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'aic' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.aic'),
    },
    outputChannelName: 'AICore Language Server',
  };

  client = new LanguageClient('aic-language-server', 'AICore Language Server', serverOptions, clientOptions);

  switch (trace) {
    case 'messages':
      client.setTrace(Trace.Messages);
      break;
    case 'verbose':
      client.setTrace(Trace.Verbose);
      break;
    default:
      client.setTrace(Trace.Off);
      break;
  }

  context.subscriptions.push(client.start());
  await client.onReady();
}

async function stopClient(): Promise<void> {
  if (!client) {
    return;
  }
  const current = client;
  client = undefined;
  await current.stop();
}
