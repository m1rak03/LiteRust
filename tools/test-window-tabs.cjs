const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

async function run() {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
    const html = fs.readFileSync('ui/toolbar.html', 'utf8')
      .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync('ui/appearance.js', 'utf8'));
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.setContent(html);
    await page.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: message => sent.push(JSON.parse(message)) };
      window.browserUpdate({
        tabs: [{ id: 1, title: 'One' }, { id: 2, title: 'Two' }, { id: 3, title: 'Three' }],
        active: 1, live_ids: [1, 2, 3], url: '', window_count: 2
      });
    });

    await page.locator('[data-tab-id="1"]').dragTo(page.locator('[data-tab-id="3"]'), {
      targetPosition: { x: 8, y: 12 }
    });
    assert.deepEqual(await page.evaluate(() => sent.at(-1)), {
      action: 'move_tab', source: 1, target: 3, after: false
    });
    await page.evaluate(() => {
      window.kept = document.querySelector('[data-tab-id="2"]');
      window.browserUpdate({ ...state, tabs: [state.tabs[1], state.tabs[0], state.tabs[2]], active: 0 });
    });
    assert.equal(await page.locator('.tab.active').getAttribute('data-tab-id'), '2');
    assert(await page.evaluate(() => kept === document.querySelector('[data-tab-id="2"]')));
    await page.locator('#more').click();
    await page.locator('#new-window').click();
    assert.equal(await page.evaluate(() => sent.at(-1).action), 'new_window');
    await page.locator('#more').click();
    await page.locator('#next-window').click();
    assert.equal(await page.evaluate(() => sent.at(-1).action), 'next_window');
    assert.deepEqual(errors, []);
    console.log('PASS: tab drag, active tab identity and ordinary window commands');
  } finally {
    await browser.close();
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
