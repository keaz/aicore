type JsonRpcId = number | string;

type LspMessage = {
  id?: JsonRpcId;
  method?: string;
  params?: Record<string, unknown>;
};

type CompletionItem = {
  label: string;
  kind: number;
  detail?: string;
};

const documents = new Map<string, string>();
let incoming = Buffer.alloc(0);

process.stdin.on('data', (chunk: Buffer | string) => {
  const payload = typeof chunk === 'string' ? Buffer.from(chunk, 'utf8') : chunk;
  incoming = Buffer.concat([incoming, payload]);
  drainIncomingBuffer();
});

process.stdin.on('end', () => {
  process.exit(0);
});

process.stdin.resume();

function drainIncomingBuffer(): void {
  while (true) {
    const headerEnd = incoming.indexOf('\r\n\r\n');
    if (headerEnd < 0) {
      return;
    }

    const header = incoming.slice(0, headerEnd).toString('utf8');
    const contentLength = parseContentLength(header);
    if (contentLength < 0) {
      incoming = incoming.slice(headerEnd + 4);
      continue;
    }

    const messageEnd = headerEnd + 4 + contentLength;
    if (incoming.length < messageEnd) {
      return;
    }

    const body = incoming.slice(headerEnd + 4, messageEnd).toString('utf8');
    incoming = incoming.slice(messageEnd);

    let message: LspMessage | undefined;
    try {
      message = JSON.parse(body) as LspMessage;
    } catch {
      continue;
    }
    handleMessage(message);
  }
}

function parseContentLength(header: string): number {
  const line = header
    .split('\r\n')
    .find((entry) => entry.toLowerCase().startsWith('content-length:'));
  if (!line) {
    return -1;
  }
  const value = Number.parseInt(line.slice('content-length:'.length).trim(), 10);
  if (!Number.isFinite(value) || value < 0) {
    return -1;
  }
  return value;
}

function handleMessage(message: LspMessage): void {
  switch (message.method) {
    case 'initialize': {
      sendResponse(message.id, {
        capabilities: {
          textDocumentSync: 1,
          completionProvider: {
            resolveProvider: false,
            triggerCharacters: ['.', ':'],
          },
        },
      });
      return;
    }
    case 'initialized':
      return;
    case 'textDocument/didOpen': {
      const textDocument = message.params?.textDocument as
        | { uri?: string; text?: string }
        | undefined;
      const uri = textDocument?.uri;
      if (!uri) {
        return;
      }
      const text = textDocument.text ?? '';
      documents.set(uri, text);
      publishDiagnostics(uri, text);
      return;
    }
    case 'textDocument/didChange': {
      const params = message.params as
        | { textDocument?: { uri?: string }; contentChanges?: Array<{ text?: string }> }
        | undefined;
      const uri = params?.textDocument?.uri;
      if (!uri) {
        return;
      }
      const changes = params?.contentChanges ?? [];
      const latestChange = changes.length > 0 ? changes[changes.length - 1] : undefined;
      const latest = latestChange?.text ?? documents.get(uri) ?? '';
      documents.set(uri, latest);
      publishDiagnostics(uri, latest);
      return;
    }
    case 'textDocument/completion': {
      const items: CompletionItem[] = [
        { label: 'fn', kind: 14, detail: 'keyword' },
        { label: 'mockComplete', kind: 3, detail: 'mock symbol' },
      ];
      sendResponse(message.id, {
        isIncomplete: false,
        items,
      });
      return;
    }
    case 'shutdown':
      sendResponse(message.id, null);
      return;
    case 'exit':
      process.exit(0);
      return;
    default:
      if (message.id !== undefined) {
        sendResponse(message.id, null);
      }
  }
}

function publishDiagnostics(uri: string, source: string): void {
  const hasErrorMarker = source.includes('!!invalid!!');
  const diagnostics = hasErrorMarker
    ? [
        {
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 11 },
          },
          severity: 1,
          source: 'aic-mock',
          message: 'mock parse error',
        },
      ]
    : [];

  sendNotification('textDocument/publishDiagnostics', {
    uri,
    diagnostics,
  });
}

function sendResponse(id: JsonRpcId | undefined, result: unknown): void {
  if (id === undefined) {
    return;
  }
  send({
    jsonrpc: '2.0',
    id,
    result,
  });
}

function sendNotification(method: string, params: unknown): void {
  send({
    jsonrpc: '2.0',
    method,
    params,
  });
}

function send(message: Record<string, unknown>): void {
  const body = JSON.stringify(message);
  const bytes = Buffer.byteLength(body, 'utf8');
  process.stdout.write(`Content-Length: ${bytes}\r\n\r\n${body}`);
}
