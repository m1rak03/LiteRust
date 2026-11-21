const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const settings = await browser.newPage({ viewport: { width: 670, height: 750 } });
    await settings.setContent(fs.readFileSync('ui/toolbar.html', 'utf8')
      .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync('ui/appearance.js', 'utf8')));
    await settings.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: json => sent.push(JSON.parse(json)) };
      window.browserUpdate({ tabs: [{ id: 1, title: 'Fixture' }], active: 0, live_ids: [1] });
      window.openPanel('settings');
    });
    await settings.locator('[data-settings-category=performance]').click();
    const selector = settings.locator('#resource-mode');
    assert.equal(await selector.inputValue(), 'manual');
    for (const mode of ['saver', 'performance', 'automatic', 'manual']) {
      await selector.selectOption(mode);
      assert.deepEqual(await settings.evaluate(() => sent.at(-1)), { action: 'set_resource_mode', mode });
    }
    await settings.locator('#resource-refresh').click();
    assert.deepEqual(await settings.evaluate(() => sent.at(-1)), { action: 'sample_resources' });
    await settings.evaluate(() => window.resourcesUpdate([
      { total: 4, visible: 2, background: 2, pinned: 1, idle: 1, playing_audio: 1, downloading: 0 },
      { process_count: 6, private_bytes: 104857600, memory_coverage: 5, cpu_percent: 3, cpu_coverage: 6 },
    ]));
    assert.equal(await settings.locator('[data-metric=visible]').textContent(), '2');
    assert.equal(await settings.locator('[data-metric=private_bytes]').textContent(), '100 MiB (5/6)');
    assert.equal(await settings.locator('[data-metric=cpu_percent]').textContent(), '3%');
    assert.equal(await settings.locator('#resource-refresh').isEnabled(), true);

    const activity = fs.readFileSync('ui/page-activity.js', 'utf8');
    const page = await browser.newPage();
    await page.setContent('<input aria-label="Draft"><main style="height:5000px">Example</main>');
    await page.addScriptTag({content: activity});
    const release = () => page.evaluate(() => window.__liteRustReleasePosition());
    await page.evaluate(() => window.scrollTo(0,200));
    assert.deepEqual(await release(), [0,200]);
    await page.evaluate(() => {
      const video = document.createElement('video');
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = 16;
      const context = canvas.getContext('2d');
      context.fillRect(0,0,16,16);
      video.muted = true;
      video.srcObject = canvas.captureStream(10);
      document.body.append(video);
      const drawing = setInterval(() => context.fillRect(0,0,16,16), 30);
      const deadline = new Promise((_, reject) => setTimeout(() => reject(new Error('Video did not play')), 1500));
      return Promise.race([video.play(), deadline]).finally(() => clearInterval(drawing));
    });
    assert.equal(await release(), null);
    await page.evaluate(() => document.querySelector('video').remove());
    assert.deepEqual(await release(), [0,200]);
    await page.getByRole('textbox', { name: 'Draft' }).fill('Unsaved');
    assert.equal(await release(), null);
    await page.getByRole('textbox', { name: 'Draft' }).fill('');
    assert.equal(await release(), null);
    console.log('PASS: memory modes, partial metrics, live muted video and unsaved form protection');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
