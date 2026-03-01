import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';

const EXTENSION_ID = 'keaz.aic-language-tools';

async function waitFor<T>(
  label: string,
  probe: () => Promise<T | undefined>,
  timeoutMs = 15_000,
  intervalMs = 200
): Promise<T> {
  const started = Date.now();
  while (Date.now() - started <= timeoutMs) {
    const result = await probe();
    if (result !== undefined) {
      return result;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`Timed out waiting for ${label} after ${timeoutMs}ms`);
}

suite('AICore VSCode Extension Integration', () => {
  let extension: vscode.Extension<unknown>;
  let workspaceRoot: string;
  let mainUri: vscode.Uri;
  let invalidUri: vscode.Uri;

  suiteSetup(async () => {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    assert.ok(workspaceFolder, 'expected integration test workspace to be opened');
    workspaceRoot = workspaceFolder.uri.fsPath;

    const loadedExtension = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(loadedExtension, `expected extension ${EXTENSION_ID} to be discoverable`);
    extension = loadedExtension;

    const mockServerPath = path.join(
      extension.extensionPath,
      'dist',
      'test',
      'fixtures',
      'mockAicLsp.js'
    );
    assert.ok(fs.existsSync(mockServerPath), `mock LSP server missing: ${mockServerPath}`);

    const config = vscode.workspace.getConfiguration('aic');
    await config.update('server.path', 'node', vscode.ConfigurationTarget.Global);
    await config.update('server.args', [mockServerPath], vscode.ConfigurationTarget.Global);
    await config.update('trace.server', 'off', vscode.ConfigurationTarget.Global);

    mainUri = vscode.Uri.file(path.join(workspaceRoot, 'main.aic'));
    invalidUri = vscode.Uri.file(path.join(workspaceRoot, 'invalid.aic'));

    const mainDocument = await vscode.workspace.openTextDocument(mainUri);
    await vscode.window.showTextDocument(mainDocument);
    await extension.activate();
  });

  suiteTeardown(async () => {
    const config = vscode.workspace.getConfiguration('aic');
    await config.update('server.path', undefined, vscode.ConfigurationTarget.Global);
    await config.update('server.args', undefined, vscode.ConfigurationTarget.Global);
    await config.update('trace.server', undefined, vscode.ConfigurationTarget.Global);
  });

  test('extension activates on .aic file open', async () => {
    const doc = await vscode.workspace.openTextDocument(mainUri);
    await vscode.window.showTextDocument(doc);
    assert.equal(doc.languageId, 'aic');
    assert.equal(extension.isActive, true);
  });

  test('LSP client starts and returns completion items', async () => {
    const completionList = await waitFor(
      'completion response',
      async () => {
        const result = await vscode.commands.executeCommand<vscode.CompletionList>(
          'vscode.executeCompletionItemProvider',
          mainUri,
          new vscode.Position(0, 0)
        );
        if (!result || result.items.length === 0) {
          return undefined;
        }
        return result;
      },
      20_000
    );

    const labels = completionList.items.map((item) => String(item.label));
    assert.ok(
      labels.includes('mockComplete'),
      `expected completion list to include mockComplete, got [${labels.join(', ')}]`
    );
  });

  test('hover provider returns markdown docs with aic code fences', async () => {
    const hovers = await waitFor(
      'hover response',
      async () => {
        const result = await vscode.commands.executeCommand<vscode.Hover[]>(
          'vscode.executeHoverProvider',
          mainUri,
          new vscode.Position(7, 17)
        );
        if (!result || result.length === 0) {
          return undefined;
        }
        return result;
      },
      20_000
    );

    const hoverText = hovers
      .flatMap((hover) => hover.contents)
      .map((content) => {
        if (content instanceof vscode.MarkdownString) {
          return content.value;
        }
        if (typeof content === 'string') {
          return content;
        }
        const marked = content as { value?: string };
        return typeof marked.value === 'string' ? marked.value : '';
      })
      .join('\n');
    assert.ok(hoverText.includes('mockComplete'));
    assert.ok(hoverText.includes('```aic'));
  });

  test('diagnostics appear for invalid code', async () => {
    const invalidDocument = await vscode.workspace.openTextDocument(invalidUri);
    await vscode.window.showTextDocument(invalidDocument);

    const diagnostics = await waitFor(
      'mock diagnostics',
      async () => {
        const entries = vscode.languages.getDiagnostics(invalidUri);
        if (entries.length === 0) {
          return undefined;
        }
        return entries;
      },
      20_000
    );

    assert.ok(
      diagnostics.some((entry) => entry.message.includes('mock parse error')),
      'expected diagnostics to include mock parse error marker'
    );
  });

  test('Restart Language Server command restarts successfully', async () => {
    await vscode.commands.executeCommand('aic.restartLanguageServer');

    const completionList = await waitFor(
      'completion after restart',
      async () => {
        const result = await vscode.commands.executeCommand<vscode.CompletionList>(
          'vscode.executeCompletionItemProvider',
          mainUri,
          new vscode.Position(0, 0)
        );
        if (!result || result.items.length === 0) {
          return undefined;
        }
        return result;
      },
      20_000
    );

    const labels = completionList.items.map((item) => String(item.label));
    assert.ok(
      labels.includes('mockComplete'),
      `expected completion list after restart to include mockComplete, got [${labels.join(', ')}]`
    );
  });

  test('debugger contribution and launch command are registered', async () => {
    const packageJson = extension.packageJSON as {
      contributes?: {
        debuggers?: Array<{ type?: string }>;
        commands?: Array<{ command?: string }>;
      };
    };
    const debuggerTypes = (packageJson.contributes?.debuggers ?? []).map((entry) => entry.type);
    assert.ok(
      debuggerTypes.includes('aic'),
      `expected contributes.debuggers to include aic, got [${debuggerTypes.join(', ')}]`
    );

    const commandIds = (packageJson.contributes?.commands ?? []).map((entry) => entry.command);
    assert.ok(
      commandIds.includes('aic.debug.createLaunchJson'),
      'expected aic.debug.createLaunchJson command contribution to be present'
    );
  });

  test('Create launch.json command writes default AICore debug config', async () => {
    const launchDir = path.join(workspaceRoot, '.vscode');
    const launchPath = path.join(launchDir, 'launch.json');
    if (fs.existsSync(launchPath)) {
      fs.unlinkSync(launchPath);
    }
    if (fs.existsSync(launchDir) && fs.readdirSync(launchDir).length === 0) {
      fs.rmdirSync(launchDir);
    }

    await vscode.commands.executeCommand('aic.debug.createLaunchJson');

    const launchJson = await waitFor(
      'launch.json generation',
      async () => {
        if (!fs.existsSync(launchPath)) {
          return undefined;
        }
        const raw = fs.readFileSync(launchPath, 'utf8');
        const parsed = JSON.parse(raw) as {
          version?: string;
          configurations?: Array<Record<string, unknown>>;
        };
        if (!Array.isArray(parsed.configurations) || parsed.configurations.length === 0) {
          return undefined;
        }
        return parsed;
      },
      20_000
    );

    assert.equal(launchJson.version, '0.2.0');
    const config = (launchJson.configurations ?? []).find(
      (entry) =>
        entry.type === 'aic' &&
        entry.request === 'launch' &&
        entry.name === 'Debug AICore'
    );
    assert.ok(config, 'expected launch.json to include Debug AICore configuration');
    assert.equal(config?.program, '${workspaceFolder}/src/main.aic');
    assert.deepEqual(config?.args, []);
  });

  test('TextMate grammar keeps keyword rules for AICore syntax highlighting', async () => {
    const grammarPath = path.join(extension.extensionPath, 'syntaxes', 'aic.tmLanguage.json');
    assert.ok(fs.existsSync(grammarPath), `missing grammar file: ${grammarPath}`);

    const grammarRaw = fs.readFileSync(grammarPath, 'utf8');
    const grammar = JSON.parse(grammarRaw) as {
      scopeName?: string;
      repository?: {
        keywords?: {
          patterns?: Array<{ name?: string; match?: string }>;
        };
      };
    };

    assert.equal(grammar.scopeName, 'source.aic');

    const keywordPatterns = grammar.repository?.keywords?.patterns ?? [];
    const controlKeywords = keywordPatterns.find((entry) => entry.name === 'keyword.control.aic');
    const storageKeywords = keywordPatterns.find((entry) => entry.name === 'storage.type.aic');

    assert.ok(
      controlKeywords?.match?.includes('if'),
      'keyword.control.aic pattern must include control-flow keyword coverage'
    );
    assert.ok(
      storageKeywords?.match?.includes('fn'),
      'storage.type.aic pattern must include fn keyword coverage'
    );
    assert.ok(
      storageKeywords?.match?.includes('struct'),
      'storage.type.aic pattern must include struct keyword coverage'
    );
  });
});
