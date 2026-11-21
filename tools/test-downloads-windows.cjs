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
  const fixture = fs.mkdtempSync(path.join(temp, 'literust-native-downloads-'));
  const size = 8 * 1024 * 1024;
  const chunk = Buffer.alloc(64 * 1024, 0x42);
  const requests = [];
  const server = http.createServer((request, response) => {
    requests.push(request.url + " " + (request.headers.range || "full"));
    if (!['/complete', '/cancel'].includes(request.url)) {
      response.writeHead(404).end();
      return;
    }
    const range = /^bytes=(\\d+)-$/i.exec(request.headers.range || '');
    let sent = range ? Number(range[1]) : 0;
    if (sent >= size) {
      response.writeHead(416, { 'Content-Range': 'bytes */' + size }).end();
      return;
    }
    const headers = {
      'Content-Type': 'application/octet-stream',
      'Content-Disposition': 'attachment; filename="' + request.url.slice(1) + '.bin"',
      'Content-Length': size - sent,
      'Accept-Ranges': 'bytes',
      'ETag': '"literust-download-fixture"',
    };
    if (range) headers['Content-Range'] = 'bytes ' + sent + '-' + (size - 1) + '/' + size;
    response.writeHead(range ? 206 : 200, headers);
    response.flushHeaders();
    const timer = setInterval(() => {
      if (response.writableLength > chunk.length) return;
      const count = Math.min(chunk.length, size - sent);
      response.write(chunk.subarray(0, count));
      sent += count;
      if (sent === size) response.end();
    }, 20);
    response.on('close', () => clearInterval(timer));
  });
  let child;
  let timeout;
  try {
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    let stderr = '';
    child = spawn(executable, ['--session-self-test', 'downloads'], {
      env: {
        ...process.env, LITERUST_SESSION_TEST_ROOT: fixture,
        LITERUST_DOWNLOAD_TEST_PORT: String(server.address().port),
      },
      stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true,
    });
    child.stderr.on('data', data => { stderr += data; });
    timeout = setTimeout(() => child.kill(), 40000);
    const [exitCode] = await once(child, 'close');
    const report = fs.existsSync(path.join(fixture, 'results.log'))
      ? fs.readFileSync(path.join(fixture, 'results.log'), 'utf8') : 'No test report';
    assert.equal(exitCode, 0, report + '\
' + stderr + requests.join(', '));
    assert.match(report, /PASS: native pause, resume, completion, cancellation and callback cleanup/);
    assert.equal(fs.statSync(path.join(fixture, 'files', 'complete.bin')).size, size);
    assert(!fs.existsSync(path.join(fixture, 'files', 'cancel.bin')));
    console.log(report.trimEnd());
  } finally {
    clearTimeout(timeout);
    if (child?.exitCode === null) child.kill();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    const resolved = fs.realpathSync(fixture);
    if (path.dirname(resolved) === temp && path.basename(resolved).startsWith('literust-native-downloads-')) {
      await fs.promises.rm(resolved, { recursive: true, force: true, maxRetries: 10, retryDelay: 250 });
    }
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
