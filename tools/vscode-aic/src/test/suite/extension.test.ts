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
