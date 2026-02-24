import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { spawnSync } from 'node:child_process';
import type {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let lspModule: typeof import('vscode-languageclient/node') | undefined;
let outputChannel: vscode.OutputChannel | undefined;
let statusBarItem: vscode.StatusBarItem | undefined;
let statusBarState: LanguageServerStatus = 'stopped';
let diagnosticsSummary: DiagnosticSummary = { errors: 0, warnings: 0 };
let serverVersion = 'unknown';
let statusDetail = '';
let stoppingClient = false;

type LanguageServerStatus = 'starting' | 'running' | 'error' | 'stopped';

type DiagnosticSummary = {
  errors: number;
  warnings: number;
};

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  getOutputChannel(context);
  getStatusBarItem(context);
  logLine(`Activating AICore extension v${String(context.extension.packageJSON.version ?? 'unknown')}`);
  setStatusBarState('starting', 'Activating extension');

  context.subscriptions.push(
    vscode.languages.onDidChangeDiagnostics(() => {
      diagnosticsSummary = collectDiagnosticsSummary();
      renderStatusBar();
    })
  );
  await startClient(context);

  const showOutput = vscode.commands.registerCommand('aic.showLanguageServerOutput', () => {
    getOutputChannel(context).show(true);
  });

  const restart = vscode.commands.registerCommand('aic.restartLanguageServer', async () => {
    logLine('Restart command received.');
    await stopClient();
    await startClient(context);
    void vscode.window.showInformationMessage('AICore language server restarted.');
  });

  context.subscriptions.push(showOutput, restart);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  if (client) {
    logLine('Language server is already running.');
    setStatusBarState('running', 'Language server already running');
    return;
  }

  setStatusBarState('starting', 'Starting language server');
  const lsp = await loadLspModule();
  if (!lsp) {
    logLine('Language client module could not be loaded.');
    setStatusBarState('error', 'Language client module could not be loaded');
    return;
  }

  const cfg = vscode.workspace.getConfiguration('aic');
  const configuredCommand = cfg.get<string>('server.path', 'aic');
  const args = cfg.get<string[]>('server.args', ['lsp']);
  const trace = cfg.get<string>('trace.server', 'off');
  const command = resolveServerCommand(configuredCommand);

  logLine(
    `Starting language server with configured path "${configuredCommand}" and args ${JSON.stringify(args)}.`
  );

  if (!command) {
    const message =
      `AICore language server executable was not found for "aic.server.path": "${configuredCommand}". ` +
      'Set "aic.server.path" to an absolute path to the aic binary.';
    logLine(message);
    setStatusBarState('error', 'Language server executable was not found');
    void vscode.window.showErrorMessage(message);
    return;
  }

  serverVersion = readServerVersion(command);
  logLine(`Detected language server version: ${serverVersion}`);
  logLine(`Resolved server executable: ${command}`);

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
    outputChannel: getOutputChannel(context),
  };

  client = new lsp.LanguageClient('aic-language-server', 'AICore Language Server', serverOptions, clientOptions);
  client.onDidChangeState((event) => {
    if (event.newState === lsp.State.Running) {
      setStatusBarState('running', 'Language server ready');
      return;
    }
    if (event.newState === lsp.State.Stopped) {
      if (stoppingClient) {
        setStatusBarState('stopped', 'Language server stopped');
      } else {
        setStatusBarState('error', 'Language server stopped unexpectedly');
      }
    }
  });

  switch (trace) {
    case 'messages':
      client.setTrace(lsp.Trace.Messages);
      break;
    case 'verbose':
      client.setTrace(lsp.Trace.Verbose);
      break;
    default:
      client.setTrace(lsp.Trace.Off);
      break;
  }

  context.subscriptions.push(client);
  try {
    logLine(`Launching: ${command} ${args.join(' ')}`.trim());
    await client.start();
    logLine('AICore language server started.');
    setStatusBarState('running', 'Language server running');
  } catch (error) {
    client = undefined;
    const detail = error instanceof Error ? error.message : String(error);
    logLine(`Failed to start language server: ${detail}`);
    setStatusBarState('error', `Failed to start language server: ${detail}`);
    void vscode.window.showErrorMessage(
      `Failed to start AICore language server (${command} ${args.join(' ')}): ${detail}`
    );
  }
}

async function stopClient(): Promise<void> {
  if (!client) {
    logLine('Stop requested, but language server is not running.');
    setStatusBarState('stopped', 'Language server stopped');
    return;
  }
  const current = client;
  client = undefined;
  stoppingClient = true;
  try {
    await current.stop();
    logLine('AICore language server stopped.');
  } finally {
    stoppingClient = false;
    setStatusBarState('stopped', 'Language server stopped');
  }
}

async function loadLspModule(): Promise<typeof import('vscode-languageclient/node') | undefined> {
  if (lspModule) {
    return lspModule;
  }

  try {
    lspModule = await import('vscode-languageclient/node');
    return lspModule;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    const message =
      'AICore extension failed to load vscode-languageclient. Repackage without "--no-dependencies" so runtime modules are included.';
    logLine(`${message} (${detail})`);
    void vscode.window.showErrorMessage(`${message} (${detail})`);
    return undefined;
  }
}

function resolveServerCommand(configuredCommand: string): string | undefined {
  const expanded = expandHome(configuredCommand.trim());
  if (!expanded) {
    return undefined;
  }

  if (path.isAbsolute(expanded)) {
    return isExecutable(expanded) ? expanded : undefined;
  }

  if (expanded.includes('/') || expanded.includes('\\')) {
    const resolved = path.resolve(expanded);
    return isExecutable(resolved) ? resolved : undefined;
  }

  return findOnPath(expanded);
}

function expandHome(p: string): string {
  if (p === '~') {
    return os.homedir();
  }
  if (p.startsWith('~/') || p.startsWith('~\\')) {
    return path.join(os.homedir(), p.slice(2));
  }
  return p;
}

function findOnPath(command: string): string | undefined {
  const pathValue = process.env.PATH;
  if (!pathValue) {
    return undefined;
  }

  const paths = pathValue.split(path.delimiter).filter(Boolean);
  const isWindows = process.platform === 'win32';
  const hasExtension = path.extname(command) !== '';
  const pathext = (process.env.PATHEXT ?? '.EXE;.CMD;.BAT;.COM')
    .split(';')
    .map((ext) => ext.toLowerCase());

  for (const dir of paths) {
    if (isWindows && !hasExtension) {
      for (const ext of pathext) {
        const candidate = path.join(dir, `${command}${ext}`);
        if (isExecutable(candidate)) {
          return candidate;
        }
      }
      continue;
    }

    const candidate = path.join(dir, command);
    if (isExecutable(candidate)) {
      return candidate;
    }
  }

  return undefined;
}

function isExecutable(p: string): boolean {
  try {
    const stats = fs.statSync(p);
    if (!stats.isFile()) {
      return false;
    }

    if (process.platform === 'win32') {
      return true;
    }

    fs.accessSync(p, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function getOutputChannel(context?: vscode.ExtensionContext): vscode.OutputChannel {
  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel('AICore Language Server');
    if (context) {
      context.subscriptions.push(outputChannel);
    }
  }
  return outputChannel;
}

function logLine(message: string): void {
  const timestamp = new Date().toISOString();
  getOutputChannel().appendLine(`[${timestamp}] ${message}`);
}

function getStatusBarItem(context?: vscode.ExtensionContext): vscode.StatusBarItem {
  if (!statusBarItem) {
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    if (context) {
      context.subscriptions.push(statusBarItem);
    }
  }
  return statusBarItem;
}

function setStatusBarState(nextState: LanguageServerStatus, detail?: string): void {
  statusBarState = nextState;
  statusDetail = detail ?? '';
  diagnosticsSummary = collectDiagnosticsSummary();
  renderStatusBar();
}

function collectDiagnosticsSummary(): DiagnosticSummary {
  let errors = 0;
  let warnings = 0;
  for (const [uri, diagnostics] of vscode.languages.getDiagnostics()) {
    if (!uri.path.endsWith('.aic') && !uri.fsPath.endsWith('.aic')) {
      continue;
    }
    for (const diagnostic of diagnostics) {
      if (diagnostic.severity === vscode.DiagnosticSeverity.Error) {
        errors += 1;
      } else if (diagnostic.severity === vscode.DiagnosticSeverity.Warning) {
        warnings += 1;
      }
    }
  }
  return { errors, warnings };
}

function renderStatusBar(): void {
  const item = getStatusBarItem();
  const hasProblems = diagnosticsSummary.errors > 0 || diagnosticsSummary.warnings > 0;

  if (statusBarState === 'starting') {
    item.text = '$(loading~spin) AICore';
    item.color = new vscode.ThemeColor('terminal.ansiYellow');
    item.command = 'aic.showLanguageServerOutput';
  } else if (statusBarState === 'error') {
    item.text = '$(error) AICore';
    item.color = new vscode.ThemeColor('terminal.ansiRed');
    item.command = 'aic.restartLanguageServer';
  } else if (statusBarState === 'stopped') {
    item.text = '$(circle-slash) AICore';
    item.color = new vscode.ThemeColor('statusBarItem.warningForeground');
    item.command = 'aic.restartLanguageServer';
  } else if (hasProblems) {
    const count = diagnosticsSummary.errors > 0 ? diagnosticsSummary.errors : diagnosticsSummary.warnings;
    const label = diagnosticsSummary.errors > 0 ? 'errors' : 'warnings';
    item.text = `$(warning) AICore: ${count} ${label}`;
    item.color = new vscode.ThemeColor('statusBarItem.warningForeground');
    item.command = 'aic.showLanguageServerOutput';
  } else {
    item.text = '$(check) AICore';
    item.color = new vscode.ThemeColor('terminal.ansiGreen');
    item.command = 'aic.showLanguageServerOutput';
  }

  const tooltipLines = [
    `AICore language server: ${statusBarState}`,
    `Version: ${serverVersion}`,
    `Diagnostics: ${diagnosticsSummary.errors} errors, ${diagnosticsSummary.warnings} warnings`,
  ];
  if (statusDetail) {
    tooltipLines.push(`Detail: ${statusDetail}`);
  }
  item.tooltip = tooltipLines.join('\n');
  item.show();
}

function readServerVersion(command: string): string {
  try {
    const output = spawnSync(command, ['--version'], {
      encoding: 'utf8',
      timeout: 2000,
      env: process.env,
    });
    if (output.status !== 0) {
      return 'unknown';
    }
    const text = `${output.stdout ?? ''}\n${output.stderr ?? ''}`
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line.length > 0);
    return text ?? 'unknown';
  } catch {
    return 'unknown';
  }
}
