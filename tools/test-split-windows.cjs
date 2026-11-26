const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { once } = require('node:events');

async function run() {
  const executable = fs.realpathSync(process.argv[2]);
  const temp = fs.realpathSync(process.env.LITERUST_TEST_TEMP || os.tmpdir());
  const root = fs.mkdtempSync(path.join(temp, 'literust-split-'));
  const server = http.createServer((request, response) => {
    const pages = { '/one': 'First', '/two': 'Second', '/three': 'Changed' };
    const title = pages[request.url];
    if (!title) { response.writeHead(404).end(); return; }
    response.writeHead(200, { 'Content-Type': 'text/html', 'Cache-Control': 'no-store' });
    response.end('<!doctype html><title>'+title+'</title><main style="height:5000px">'+title+'</main>');
  });
  try {
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    for (const phase of ['split', 'split-private']) {
      const fixture = path.join(root, phase);
      fs.mkdirSync(fixture);
      let stderr = '';
      const child = spawn(executable, ['--session-self-test', phase], {
        env: {
          ...process.env, LOCALAPPDATA: fixture, LITERUST_SESSION_TEST_ROOT: fixture,
          LITERUST_SPLIT_TEST_PORT: String(server.address().port),
        },
        stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true,
      });
      child.stderr.on('data', data => { stderr += data; });
      const timeout = setTimeout(() => child.kill(), 50000);
      try {
        const [exitCode] = await once(child, 'close');
        const reportPath = path.join(fixture, 'results.log');
        const report = fs.existsSync(reportPath) ? fs.readFileSync(reportPath, 'utf8') : 'No test report';
        assert.equal(exitCode, 0, report + '\n' + stderr);
        assert.match(report, /PASS: Settings reuse, switch, closure and home WebView retention on recolor/);
        assert.match(report, /PASS: separate native WebViews/);
        assert.match(report, /PASS: workspace switching retained the original WebViews/);
        assert.match(report, /PASS: note editing and page links, native selection capture, and native Save As HTML/);
        assert.match(report, /PASS: process metrics, edited and pinned tab protection, manual release and scroll restoration/);
        assert.match(report, /PASS: real WebView2 background policy, protected tabs and continued page scripts/);
        assert.match(report, /PASS: diagnostic report, private log isolation and exact preview export/);
        console.log(phase + '\n' + report.trimEnd());
      } finally {
        clearTimeout(timeout);
        if (child.exitCode === null) child.kill();
      }
    }
  } finally {
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    const resolved = fs.realpathSync(root);
    if (path.dirname(resolved) === temp && path.basename(resolved).startsWith('literust-split-')) {
      await fs.promises.rm(resolved, { recursive: true, force: true, maxRetries: 10, retryDelay: 300 });
    }
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
