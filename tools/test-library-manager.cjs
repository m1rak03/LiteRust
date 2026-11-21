const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

async function run() {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 690, height: 600 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('dialog', dialog => dialog.accept());
    const html = fs.readFileSync('ui/toolbar.html', 'utf8')
      .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync('ui/appearance.js', 'utf8'));
    await page.setContent(html);
    await page.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: message => sent.push(JSON.parse(message)) };
      window.openPanel('downloads-manager');
      window.entries = [
        { id: 1, name: 'one.zip', source: 'https://example.com/one', has_folder: true,
          status: 'running', progress: { received: 50, total: 100, can_pause: true, can_cancel: true } },
        { id: 2, name: '<b>two.txt</b>', has_folder: true, status: 'completed',
          source: 'https://example.com/two', progress: { received: 100, total: 100 } },
      ];
      window.downloadManagerUpdate(entries);
    });
    const entry = page.locator('[data-download-id="1"]');
    await page.waitForTimeout(500);
    assert.equal(await page.evaluate(() => sent.some(message => message.action === 'download_refresh')), false);
    await page.evaluate(() => {
      entries[0].progress.received = 80;
      window.downloadManagerUpdate(entries);
    });
    assert.equal(await entry.locator('progress').getAttribute('value'), '80');
    await entry.getByRole('button', { name: 'Пауза' }).click();
    assert.deepEqual(await page.evaluate(() => sent.find(m => m.action === 'download_control')),
      { action: 'download_control', id: 1, operation: 'pause' });
    await page.evaluate(() => {
      entries[0].status = 'paused';entries[0].progress.can_pause = false;
      entries[0].progress.can_resume = true;window.downloadManagerUpdate(entries);
    });
    await entry.getByRole('button', { name: 'Возобновить' }).click();
    await entry.getByRole('button', { name: 'Отменить загрузку' }).click();
    await page.locator('#download-filter').selectOption('completed');
    assert.equal(await page.locator('#download-list .library-record:visible').count(), 1);
    assert.equal(await page.locator('[data-download-id="2"] strong').textContent(), '<b>two.txt</b>');
    await page.getByRole('button', { name: 'Открыть папку' }).click();
    assert.deepEqual(await page.evaluate(() => sent.at(-1)), { action: 'download_folder', id: 2 });
    await page.locator('#download-search').fill('nothing');
    assert(await page.locator('#download-empty').isVisible());

    await page.evaluate(() => {
      window.openPanel('bookmarks-manager');
      window.bookmarkManagerUpdate({
        folders: ['Work'],
        items: [
          { url: 'https://example.com/alpha', title: 'Alpha', added_at: 2 },
          { url: 'https://example.com/beta', title: 'Beta', folder: 'Work', added_at: 3 },
        ],
      });
    });
    await page.locator('#bookmark-sort').selectOption('title');
    assert.deepEqual(await page.locator('#bookmark-list .library-open').allTextContents(), ['Alpha','Beta']);
    await page.locator('#bookmark-folder-filter').selectOption('folder:Work');
    assert.equal(await page.locator('#bookmark-list .library-open').textContent(), 'Beta');
    await page.getByRole('button', { name: 'Редактировать', exact: true }).click();
    await page.locator('#bookmark-title').fill('Edited');
    await page.locator('#bookmark-destination').selectOption('');
    await page.locator('#bookmark-editor button[type=submit]').click();
    assert.deepEqual(await page.evaluate(() => sent.at(-1)),
      { action: 'bookmark_update', url: 'https://example.com/beta', title: 'Edited', folder: null });
    await page.locator('#bookmark-folder-delete').click();
    assert.deepEqual(await page.evaluate(() => sent.at(-1)), { action: 'bookmark_folder', previous: 'Work' });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.deepEqual(errors, []);
    console.log('PASS: download controls, filtering and open-folder command; bookmark sorting, editing and folders');
  } finally {
    await browser.close();
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
