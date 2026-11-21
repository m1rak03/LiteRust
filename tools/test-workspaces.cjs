const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 690, height: 620 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const html = fs.readFileSync('ui/toolbar.html', 'utf8')
      .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync('ui/appearance.js', 'utf8'));
    await page.setContent(html);
    await page.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: message => sent.push(JSON.parse(message)) };
      window.workspaceFixture = {
        tabs: [{ id: 31, title: 'Ordinary' }], active: 0, live_ids: [31],
        url: 'https://ordinary.example/', current_workspace: 0, window_count: 1,
        customization: { compact_tabs: false, accent: 0x110022 },
        workspaces: [{id: 1, name: '<img src=x>', display: {accent: 0x116688, compact_tabs: true}}]
      };
      window.browserUpdate(workspaceFixture);
    });
    const latest = action => page.evaluate(action => sent.filter(message => message.action === action).at(-1), action);
    await page.locator('#workspace-switcher').click();
    await page.locator('#workspace-create input').fill('  Work  ');
    await page.locator('#workspace-create button').click();
    assert.deepEqual(await latest('workspace_create'), {action:'workspace_create',name:'Work'});
    const existing = page.locator('button[data-workspace-id="1"]');
    assert.equal(await existing.textContent(), '<img src=x>');
    assert.equal(await existing.locator('img').count(), 0);
    await existing.click();
    assert.deepEqual(await latest('workspace_switch'), {action:'workspace_switch',id:1});

    await page.evaluate(() => {
      window.workspaceChanged();
      window.browserUpdate({...workspaceFixture, current_workspace:1,
        tabs:[{id:42,title:'Workspace tab'}], live_ids:[42], url:'https://work.example/'});
    });
    assert.equal(await page.locator('#workspace-label').textContent(), '<img src=x>');
    assert.equal(await page.locator('body').evaluate(body => body.classList.contains('compact-tabs')), true);
    assert.equal(await page.locator('#pref-accent').inputValue(), '#110022');
    await page.locator('#workspace-switcher').click();
    assert.equal(await existing.getAttribute('aria-current'), 'true');
    await page.locator('#workspace-name').fill('Study');
    await page.locator('#workspace-density').selectOption('regular');
    await page.locator('#workspace-accent').fill('#113355');
    await page.locator('#workspace-editor button').click();
    assert.deepEqual(await latest('workspace_update'), {
      action:'workspace_update', id:1, name:'Study', display:{compact_tabs:false,accent:0x113355}
    });
    await page.locator('button[data-workspace-id="0"]').click();
    assert.deepEqual(await latest('workspace_switch'), {action:'workspace_switch',id:0});
    assert.equal(await page.locator('#workspaces').isVisible(), false);
    assert.deepEqual(errors, []);
    console.log('PASS: workspace creation, selection, safe labels and profile-independent display options');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
