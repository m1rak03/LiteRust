const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 690, height: 630 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.setContent(fs.readFileSync('ui/toolbar.html','utf8')
      .replace('/* APPEARANCE_SCRIPT */',fs.readFileSync('ui/appearance.js','utf8')));
    await page.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: message => sent.push(JSON.parse(message)) };
      window.browserUpdate({
        tabs: [{ id: 1, title: 'First' }, { id: 2, title: 'Second' }, { id: 3, title: 'Third' }],
        active: 0, live_ids: [1, 2], url: 'https://first.example/', window_count: 1,
      });
    });
    const latest = () => page.evaluate(() => sent.filter(message => message.action === 'split').at(-1)?.operation);
    await page.locator('[data-tab-id="2"]').click({button:'right'});
    await page.locator('#split-open').click();
    assert.deepEqual(await latest(), {kind:'open',tab:2});
    await page.evaluate(() => window.browserUpdate({
      ...state, split:{panes:[1,2],focus:'first',axis:'horizontal',percent:50,
        urls:['https://first.example/','https://second.example/']}
    }));
    assert(!(await page.locator('#address').isVisible()));
    assert.equal(await page.locator('.split-pane[data-active=true]').getAttribute('data-pane'),'first');
    const pane = name => page.locator('.split-pane[data-pane="'+name+'"]');
    const second = pane('second').locator('input');
    assert.equal(await second.inputValue(), 'https://second.example/');
    await second.fill('https://new.example/');
    await second.press('Enter');
    assert.deepEqual(await latest(),{kind:'navigate',pane:'second',value:'https://new.example/'});
    assert.equal(await pane('first').locator('input').inputValue(), 'https://first.example/');
    await pane('first').locator('select').selectOption('3');
    assert.deepEqual(await latest(),{kind:'choose',pane:'first',tab:3});
    await page.locator('#split-axis').selectOption('vertical');
    assert.deepEqual(await latest(),{kind:'axis',axis:'vertical'});
    await page.locator('#split-ratio').evaluate(element => {
      element.value = 33;element.dispatchEvent(new Event('change'));
    });
    assert.deepEqual(await latest(),{kind:'resize',percent:33});
    await page.locator('#split-swap').click();
    assert.deepEqual(await latest(),{kind:'swap'});
    await pane('second').locator('button').click();
    assert.deepEqual(await latest(),{kind:'close',pane:'second'});
    await page.locator('#split-exit').click();
    assert.deepEqual(await latest(),{kind:'exit'});
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    if(process.env.LITERUST_SPLIT_TEST_SCREENSHOT) await page.screenshot({path:process.env.LITERUST_SPLIT_TEST_SCREENSHOT});
    await page.evaluate(() => window.browserUpdate({ ...state, split:null }));
    assert(await page.locator('#address').isVisible());
    assert.deepEqual(errors, []);
    console.log('PASS: split context action, separate addresses, tab chooser, resize, swap and ordinary mode');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
