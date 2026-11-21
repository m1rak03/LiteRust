const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fixture = require('./rust-fixture.cjs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1200, height: 760 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.setContent(fixture('newtab-preview'));
    assert.equal(await page.locator('.sign-in').getAttribute('href'), 'literust://accounts');
    assert(await page.locator('.shortcut').count() >= 2);
    assert.equal(await page.locator('.customize').getAttribute('href'), 'literust://appearance');
    assert.equal(await page.locator('#q').getAttribute('placeholder'), 'Что хотите найти?');
    await page.setViewportSize({ width: 690, height: 700 });
    assert(await page.locator('.sign-in').isVisible());
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.evaluate(() => {
      window.commands = [];
      window.homeSend = command => commands.push(command);
    });
    await page.locator('#q').fill('привет & тест');
    await page.locator('#q').press('Enter');
    assert.deepEqual(await page.evaluate(() => commands.at(-1)), {
      action: 'navigate', value: 'привет & тест',
    });
    assert.deepEqual(errors, []);
    console.log('PASS: rendered new tab, profile entry, search action and narrow layout');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
