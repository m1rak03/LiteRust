const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 690, height: 720 } });
    await page.setContent(fs.readFileSync('ui/toolbar.html','utf8')
      .replace('/* APPEARANCE_SCRIPT */',fs.readFileSync('ui/appearance.js','utf8')));
    await page.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: json => sent.push(JSON.parse(json)) };
      window.browserUpdate({ tabs: [{ id: 7, title: 'Fixture' }], active: 0, live_ids: [7] });
      window.openPanel('settings');
    });
    await page.locator('[data-settings-category=diagnostics]').click();
    assert.deepEqual(await page.evaluate(() => sent.at(-1)), { action: 'read_diagnostics', preview: false });
    assert.equal(await page.locator('#diagnostics-export').isEnabled(), false);
    const report = JSON.stringify({
      browser_version: '0.18.0-alpha.4', rust_build: 'rustc 1.100.0', runtime: '157.0.4341.60',
      active_profiles: 1, vpn: 'off', journal: 'local', workspace_tabs: 1, live_views: 1,
      events: [
        { timestamp: 1700000000, component: 'network', event: 'network_rules_failed', error: true },
        { timestamp: 1700000000, component: 'windows', event: 'window_opened', error: false },
      ],
    }, null, 2);
    await page.evaluate(serialized => window.diagnosticsUpdate([serialized, ['<b>Profile</b>'], false]), report);
    assert.equal(await page.locator('[data-diag=active_profiles]').textContent(), '<b>Profile</b>');
    assert.equal(await page.locator('[data-diag=active_profiles] b').count(), 0);
    assert.equal(await page.locator('#diagnostics-errors li').count(), 1);
    await page.getByText('Журнал компонентов',{exact:true}).click();
    await page.locator('#diagnostics-component').selectOption('windows');
    assert.deepEqual(await page.locator('#diagnostics-log li code').allTextContents(), ['window_opened']);
    await page.locator('#diagnostics-prepare').click();
    assert.deepEqual(await page.evaluate(() => sent.at(-1)), { action: 'read_diagnostics', preview: true });
    await page.evaluate(serialized => window.diagnosticsUpdate([serialized, ['<b>Profile</b>'], true]), report);
    assert.equal(await page.locator('#diagnostics-preview').textContent(), report);
    await page.locator('#diagnostics-export').click();
    assert.deepEqual(await page.evaluate(() => sent.at(-1)), { action: 'export_diagnostics' });
    await page.locator('#diagnostics-refresh').click();
    assert.equal(await page.locator('#diagnostics-export').isEnabled(), false);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    console.log('PASS: diagnostic statuses, recent errors, category filter, review and export from a narrow settings panel');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
