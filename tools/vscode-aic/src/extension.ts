import * as vscode from 'vscode';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
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
let pendingRestartTimer: NodeJS.Timeout | undefined;
let restartTask: Promise<void> | undefined;
let errorLensDecorations: ErrorLensDecorations | undefined;
let errorLensConfig: ErrorLensConfig = {
  enabled: true,
  showOnlyFirstPerLine: true,
  maxMessageLength: 140,
};

type LanguageServerStatus = 'starting' | 'running' | 'error' | 'stopped';

type DiagnosticSummary = {
  errors: number;
  warnings: number;
};

type ErrorLensConfig = {
  enabled: boolean;
  showOnlyFirstPerLine: boolean;
  maxMessageLength: number;
};

type ErrorLensDecorations = {
  error: vscode.TextEditorDecorationType;
  warning: vscode.TextEditorDecorationType;
  info: vscode.TextEditorDecorationType;
  hint: vscode.TextEditorDecorationType;
};

type ProcessRunResult = {
  exitCode: number;
  signal: NodeJS.Signals | null;
  stdout: string;
  stderr: string;
  errorMessage?: string;
  cancelled: boolean;
};

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  getOutputChannel(context);
  getStatusBarItem(context);
  getErrorLensDecorations(context);
  logLine(`Activating AICore extension v${String(context.extension.packageJSON.version ?? 'unknown')}`);
  setStatusBarState('starting', 'Activating extension');
  refreshErrorLensConfig();

  context.subscriptions.push(
    vscode.languages.onDidChangeDiagnostics(() => {
      diagnosticsSummary = collectDiagnosticsSummary();
      renderStatusBar();
      renderErrorLensForActiveEditor();
    }),
    vscode.window.onDidChangeActiveTextEditor(() => {
      renderErrorLensForActiveEditor();
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (
        event.affectsConfiguration('aic.server.path') ||
        event.affectsConfiguration('aic.server.args') ||
        event.affectsConfiguration('aic.trace.server')
      ) {
        queueClientRestart(context, 'AICore server settings changed');
      }
      if (
        event.affectsConfiguration('aic.errorLens.enabled') ||
        event.affectsConfiguration('aic.errorLens.showOnlyFirstPerLine')
      ) {
        refreshErrorLensConfig();
        renderErrorLensForActiveEditor();
      }
    })
  );
  renderErrorLensForActiveEditor();
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

  const createLaunchJson = vscode.commands.registerCommand('aic.debug.createLaunchJson', async () => {
    await createLaunchJsonTemplate();
  });

  const debugConfigProvider = new AicDebugConfigurationProvider();
  const debugAdapterFactory = new AicDebugAdapterDescriptorFactory();
  const debugConfigRegistration = vscode.debug.registerDebugConfigurationProvider(
    'aic',
    debugConfigProvider
  );
  const debugAdapterRegistration = vscode.debug.registerDebugAdapterDescriptorFactory(
    'aic',
    debugAdapterFactory
  );

  context.subscriptions.push(
    showOutput,
    restart,
    createLaunchJson,
    debugConfigRegistration,
    debugAdapterRegistration
  );
}

export async function deactivate(): Promise<void> {
  clearPendingRestart();
  await stopClient();
}

function clearPendingRestart(): void {
  if (!pendingRestartTimer) {
    return;
  }
  clearTimeout(pendingRestartTimer);
  pendingRestartTimer = undefined;
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

function queueClientRestart(context: vscode.ExtensionContext, reason: string): void {
  clearPendingRestart();
  pendingRestartTimer = setTimeout(() => {
    pendingRestartTimer = undefined;
    void restartClient(context, reason);
  }, 250);
}

async function restartClient(context: vscode.ExtensionContext, reason: string): Promise<void> {
  if (restartTask) {
    logLine(`Restart requested while another restart is in progress (${reason}).`);
    return restartTask;
  }

  restartTask = (async () => {
    logLine(`Restarting language server after configuration change: ${reason}.`);
    setStatusBarState('starting', reason);
    await stopClient();
    await startClient(context);
  })();

  try {
    await restartTask;
  } finally {
    restartTask = undefined;
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

  const shellResolved = findOnUserShellPath(expanded);
  const envResolved = findOnPath(expanded);
  if (shellResolved && envResolved && shellResolved !== envResolved) {
    logLine(
      `Resolved "${expanded}" from user shell as "${shellResolved}" (VS Code PATH resolves to "${envResolved}").`
    );
  }
  return shellResolved ?? envResolved;
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

function findOnUserShellPath(command: string): string | undefined {
  if (process.platform === 'win32') {
    const probe = spawnSync('where', [command], {
      encoding: 'utf8',
      windowsHide: true,
    });
    if (probe.status !== 0) {
      return undefined;
    }
    const candidate = firstNonEmptyLine(probe.stdout);
    return candidate && isExecutable(candidate) ? candidate : undefined;
  }

  const shell = (process.env.SHELL ?? (process.platform === 'darwin' ? '/bin/zsh' : '/bin/sh')).trim();
  if (!shell || !isExecutable(shell)) {
    return undefined;
  }

  const probe = spawnSync(
    shell,
    [
      '-lc',
      'cmd="$1"; IFS=":"; for dir in $PATH; do [ -z "$dir" ] && continue; candidate="$dir/$cmd"; if [ -f "$candidate" ] && [ -x "$candidate" ]; then printf "%s\\n" "$candidate"; exit 0; fi; done; exit 1',
      'aic-resolve',
      command,
    ],
    {
      encoding: 'utf8',
      windowsHide: true,
    }
  );
  if (probe.status !== 0) {
    return undefined;
  }
  const candidate = firstNonEmptyLine(probe.stdout);
  return candidate && isExecutable(candidate) ? candidate : undefined;
}

function firstNonEmptyLine(text: string): string | undefined {
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.length > 0) {
      return trimmed;
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

function refreshErrorLensConfig(): void {
  const config = vscode.workspace.getConfiguration('aic');
  errorLensConfig = {
    enabled: config.get<boolean>('errorLens.enabled', true),
    showOnlyFirstPerLine: config.get<boolean>('errorLens.showOnlyFirstPerLine', true),
    maxMessageLength: 140,
  };
}

function getErrorLensDecorations(context?: vscode.ExtensionContext): ErrorLensDecorations {
  if (!errorLensDecorations) {
    errorLensDecorations = {
      error: vscode.window.createTextEditorDecorationType({
        after: {
          margin: '0 0 0 1.5rem',
          color: new vscode.ThemeColor('problemsErrorIcon.foreground'),
        },
      }),
      warning: vscode.window.createTextEditorDecorationType({
        after: {
          margin: '0 0 0 1.5rem',
          color: new vscode.ThemeColor('problemsWarningIcon.foreground'),
        },
      }),
      info: vscode.window.createTextEditorDecorationType({
        after: {
          margin: '0 0 0 1.5rem',
          color: new vscode.ThemeColor('problemsInfoIcon.foreground'),
        },
      }),
      hint: vscode.window.createTextEditorDecorationType({
        after: {
          margin: '0 0 0 1.5rem',
          color: new vscode.ThemeColor('editorCodeLens.foreground'),
        },
      }),
    };
    if (context) {
      context.subscriptions.push(
        errorLensDecorations.error,
        errorLensDecorations.warning,
        errorLensDecorations.info,
        errorLensDecorations.hint
      );
    }
  }
  return errorLensDecorations;
}

function renderErrorLensForActiveEditor(): void {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'aic') {
    if (editor) {
      clearErrorLensForEditor(editor);
    }
    return;
  }
  renderErrorLensForEditor(editor);
}

function renderErrorLensForEditor(editor: vscode.TextEditor): void {
  const decorations = getErrorLensDecorations();
  if (!errorLensConfig.enabled) {
    clearErrorLensForEditor(editor);
    return;
  }

  const diagnostics = vscode.languages
    .getDiagnostics(editor.document.uri)
    .slice()
    .sort((lhs, rhs) => {
      if (lhs.range.start.line !== rhs.range.start.line) {
        return lhs.range.start.line - rhs.range.start.line;
      }
      return severityPriority(lhs.severity) - severityPriority(rhs.severity);
    });

  const errors: vscode.DecorationOptions[] = [];
  const warnings: vscode.DecorationOptions[] = [];
  const infos: vscode.DecorationOptions[] = [];
  const hints: vscode.DecorationOptions[] = [];
  const seenLines = new Set<number>();

  for (const diagnostic of diagnostics) {
    const line = Math.min(
      Math.max(diagnostic.range.start.line, 0),
      Math.max(editor.document.lineCount - 1, 0)
    );
    if (errorLensConfig.showOnlyFirstPerLine && seenLines.has(line)) {
      continue;
    }
    seenLines.add(line);

    const lineLength = editor.document.lineAt(line).text.length;
    const range = new vscode.Range(
      new vscode.Position(line, lineLength),
      new vscode.Position(line, lineLength)
    );
    const message = truncateErrorLensMessage(diagnostic.message, errorLensConfig.maxMessageLength);
    const contentText = ` ${diagnosticSeverityLabel(diagnostic.severity)} ${message}`;
    const option: vscode.DecorationOptions = {
      range,
      renderOptions: {
        after: {
          contentText,
        },
      },
    };

    if (diagnostic.severity === vscode.DiagnosticSeverity.Error) {
      errors.push(option);
    } else if (diagnostic.severity === vscode.DiagnosticSeverity.Warning) {
      warnings.push(option);
    } else if (diagnostic.severity === vscode.DiagnosticSeverity.Information) {
      infos.push(option);
    } else {
      hints.push(option);
    }
  }

  editor.setDecorations(decorations.error, errors);
  editor.setDecorations(decorations.warning, warnings);
  editor.setDecorations(decorations.info, infos);
  editor.setDecorations(decorations.hint, hints);
}

function clearErrorLensForEditor(editor: vscode.TextEditor): void {
  const decorations = getErrorLensDecorations();
  editor.setDecorations(decorations.error, []);
  editor.setDecorations(decorations.warning, []);
  editor.setDecorations(decorations.info, []);
  editor.setDecorations(decorations.hint, []);
}

function severityPriority(severity: vscode.DiagnosticSeverity): number {
  if (severity === vscode.DiagnosticSeverity.Error) {
    return 0;
  }
  if (severity === vscode.DiagnosticSeverity.Warning) {
    return 1;
  }
  if (severity === vscode.DiagnosticSeverity.Information) {
    return 2;
  }
  return 3;
}

function diagnosticSeverityLabel(severity: vscode.DiagnosticSeverity): string {
  if (severity === vscode.DiagnosticSeverity.Error) {
    return 'Error:';
  }
  if (severity === vscode.DiagnosticSeverity.Warning) {
    return 'Warning:';
  }
  if (severity === vscode.DiagnosticSeverity.Information) {
    return 'Info:';
  }
  return 'Hint:';
}

function truncateErrorLensMessage(message: string, maxLength: number): string {
  const normalized = message.replace(/\s+/g, ' ').trim();
  if (normalized.length <= maxLength) {
    return normalized;
  }
  if (maxLength <= 3) {
    return normalized.slice(0, maxLength);
  }
  return `${normalized.slice(0, maxLength - 3)}...`;
}

class AicDebugConfigurationProvider implements vscode.DebugConfigurationProvider {
  provideDebugConfigurations(folder: vscode.WorkspaceFolder | undefined): vscode.DebugConfiguration[] {
    return [defaultAicLaunchConfiguration(folder)];
  }

  async resolveDebugConfiguration(
    folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration
  ): Promise<vscode.DebugConfiguration | undefined> {
    const normalized = normalizeAicLaunchConfiguration(folder, config);
    const aicCommand = resolveAicExecutableForDebug();
    if (!aicCommand) {
      void vscode.window.showErrorMessage(
        'AICore debugger requires a valid "aic.server.path" executable. Set it to your aic binary.'
      );
      return undefined;
    }

    if (typeof normalized.program !== 'string' || normalized.program.trim().length === 0) {
      void vscode.window.showErrorMessage(
        'AICore debug configuration is missing "program". Use "AICore: Create launch.json".'
      );
      return undefined;
    }

    const programPath = resolvePathVariables(normalized.program, folder);
    if (programPath.endsWith('.aic')) {
      const builtProgram = await buildAicDebugTarget(aicCommand, programPath, normalized.cwd, folder);
      if (!builtProgram) {
        return undefined;
      }
      normalized.program = builtProgram;
      normalized.cwd = folder?.uri.fsPath ?? path.dirname(builtProgram);
    } else {
      normalized.program = programPath;
      if (typeof normalized.cwd === 'string' && normalized.cwd.length > 0) {
        normalized.cwd = resolvePathVariables(normalized.cwd, folder);
      }
    }

    if (normalized.breakOnContractViolation === true) {
      const current = Array.isArray(normalized.initCommands)
        ? normalized.initCommands.map((value) => String(value))
        : [];
      if (!current.some((entry) => entry.includes('aic_rt_panic'))) {
        current.push('breakpoint set --name aic_rt_panic');
      }
      normalized.initCommands = current;
    }

    return normalized;
  }
}

class AicDebugAdapterDescriptorFactory implements vscode.DebugAdapterDescriptorFactory {
  createDebugAdapterDescriptor(
    _session: vscode.DebugSession,
    _executable: vscode.DebugAdapterExecutable | undefined
  ): vscode.DebugAdapterDescriptor | undefined {
    const aicCommand = resolveAicExecutableForDebug();
    if (!aicCommand) {
      void vscode.window.showErrorMessage(
        'AICore debugger could not resolve the aic executable. Update "aic.server.path".'
      );
      return undefined;
    }

    const config = vscode.workspace.getConfiguration('aic');
    const adapterPath = config.get<string>('debug.adapterPath', '').trim();
    const args = ['debug', 'dap'];
    if (adapterPath.length > 0) {
      args.push('--adapter', resolvePathVariables(adapterPath, vscode.workspace.workspaceFolders?.[0]));
    }
    return new vscode.DebugAdapterExecutable(aicCommand, args, { env: processEnvStrings() });
  }
}

function defaultAicLaunchConfiguration(folder: vscode.WorkspaceFolder | undefined): vscode.DebugConfiguration {
  const workspaceProgram = folder ? '${workspaceFolder}/src/main.aic' : 'src/main.aic';
  return {
    type: 'aic',
    request: 'launch',
    name: 'Debug AICore',
    program: workspaceProgram,
    args: [],
    cwd: folder ? '${workspaceFolder}' : '${workspaceFolder}',
    stopOnEntry: false,
    breakOnContractViolation: false,
  };
}

function normalizeAicLaunchConfiguration(
  folder: vscode.WorkspaceFolder | undefined,
  config: vscode.DebugConfiguration
): vscode.DebugConfiguration {
  const normalized = {
    ...defaultAicLaunchConfiguration(folder),
    ...config,
  } as vscode.DebugConfiguration;
  normalized.type = 'aic';
  normalized.request = 'launch';
  if (!Array.isArray(normalized.args)) {
    normalized.args = [];
  }
  return normalized;
}

function resolveAicExecutableForDebug(): string | undefined {
  const config = vscode.workspace.getConfiguration('aic');
  const configured = config.get<string>('server.path', 'aic');
  return resolveServerCommand(configured);
}

function resolvePathVariables(input: string, folder: vscode.WorkspaceFolder | undefined): string {
  const workspacePath = folder?.uri.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  let value = expandHome(input);
  if (workspacePath) {
    value = value.replace(/\$\{workspaceFolder\}/g, workspacePath);
  }
  if (path.isAbsolute(value)) {
    return value;
  }
  return path.resolve(workspacePath ?? process.cwd(), value);
}

function processEnvStrings(): { [key: string]: string } {
  const out: { [key: string]: string } = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (typeof value === 'string') {
      out[key] = value;
    }
  }
  return out;
}

async function buildAicDebugTarget(
  aicCommand: string,
  sourceProgram: string,
  cwd: unknown,
  folder: vscode.WorkspaceFolder | undefined
): Promise<string | undefined> {
  const cwdValue =
    typeof cwd === 'string' && cwd.trim().length > 0
      ? resolvePathVariables(cwd, folder)
      : folder?.uri.fsPath ?? path.dirname(sourceProgram);
  const outDir = path.join(cwdValue, '.aic-cache', 'debug');
  fs.mkdirSync(outDir, { recursive: true });
  const outputName = `${path.parse(sourceProgram).name}${process.platform === 'win32' ? '.exe' : ''}`;
  const outputPath = path.join(outDir, outputName);

  const args = ['build', sourceProgram, '--debug-info', '-o', outputPath];
  logLine(`Starting debug pre-launch build: ${aicCommand} ${args.join(' ')}`);

  const result = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `AICore: building ${path.basename(sourceProgram)} for debug`,
      cancellable: true,
    },
    async (_progress, token) => runProcess(aicCommand, args, cwdValue, token)
  );

  if (result.cancelled) {
    logLine('Debug pre-launch build cancelled by user.');
    void vscode.window.showWarningMessage('AICore debug build was cancelled.');
    return undefined;
  }

  if (result.exitCode !== 0) {
    const details = [result.errorMessage, result.stdout.trim(), result.stderr.trim()]
      .filter((value) => typeof value === 'string' && value.length > 0)
      .join('\n');
    logLine(
      `Debug pre-launch build failed (${aicCommand} ${args.join(' ')}): ${details || 'unknown error'}`
    );
    void vscode.window.showErrorMessage(
      `AICore debug build failed. ${details || 'See AICore language server output for details.'}`
    );
    return undefined;
  }

  logLine(`Debug pre-launch build completed: ${outputPath}`);
  return outputPath;
}

function runProcess(
  command: string,
  args: string[],
  cwd: string,
  cancellationToken: vscode.CancellationToken
): Promise<ProcessRunResult> {
  return new Promise((resolve) => {
    let settled = false;
    let stdout = '';
    let stderr = '';
    let cancelled = false;
    let cancellationDisposable: vscode.Disposable | undefined;

    const finish = (result: ProcessRunResult): void => {
      if (settled) {
        return;
      }
      settled = true;
      cancellationDisposable?.dispose();
      resolve(result);
    };

    const child = spawn(command, args, {
      cwd,
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    child.stdout?.on('data', (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });
    child.stderr?.on('data', (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });

    cancellationDisposable = cancellationToken.onCancellationRequested(() => {
      cancelled = true;
      child.kill();
    });

    child.on('error', (error) => {
      finish({
        exitCode: -1,
        signal: null,
        stdout,
        stderr,
        errorMessage: error.message,
        cancelled,
      });
    });

    child.on('close', (code, signal) => {
      finish({
        exitCode: typeof code === 'number' ? code : -1,
        signal,
        stdout,
        stderr,
        cancelled,
      });
    });
  });
}

async function createLaunchJsonTemplate(): Promise<void> {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    void vscode.window.showErrorMessage('Open a workspace folder before creating launch.json.');
    return;
  }

  const launchPath = path.join(folder.uri.fsPath, '.vscode', 'launch.json');
  fs.mkdirSync(path.dirname(launchPath), { recursive: true });

  const template = defaultAicLaunchConfiguration(folder);
  let launchJson: { version: string; configurations: vscode.DebugConfiguration[] } = {
    version: '0.2.0',
    configurations: [],
  };

  if (fs.existsSync(launchPath)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(launchPath, 'utf8')) as {
        version?: string;
        configurations?: vscode.DebugConfiguration[];
      };
      launchJson = {
        version: typeof parsed.version === 'string' ? parsed.version : '0.2.0',
        configurations: Array.isArray(parsed.configurations) ? parsed.configurations : [],
      };
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      void vscode.window.showErrorMessage(`Unable to parse existing launch.json: ${detail}`);
      return;
    }
  }

  const exists = launchJson.configurations.some(
    (entry) => entry.type === 'aic' && entry.name === 'Debug AICore'
  );
  if (!exists) {
    launchJson.configurations.push(template);
  }

  fs.writeFileSync(launchPath, `${JSON.stringify(launchJson, null, 2)}${os.EOL}`, 'utf8');
  const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(launchPath));
  await vscode.window.showTextDocument(doc);
  void vscode.window.showInformationMessage('Created AICore debug launch configuration.');
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
