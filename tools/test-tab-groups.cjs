const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

async function run() {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1000, height: 650 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const html = fs.readFileSync('ui/toolbar.html', 'utf8')
      .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync('ui/appearance.js', 'utf8'));
    await page.setContent(html);
    await page.evaluate(() => {
      window.sent = [];
      window.ipc = { postMessage: message => sent.push(JSON.parse(message)) };
      window.browserUpdate({
        tabs: [{ id: 1, title: 'One' }, { id: 2, title: 'Two' }, { id: 3, title: 'Three' }],
        active: 1, live_ids: [1, 2, 3], url: '', window_count: 1
      });
    });

    const tab = id => page.locator('[data-tab-id="'+id+'"]');
    const group = id => page.locator('[data-tab-group-id="'+id+'"]');
    const latest = () => page.evaluate(() => sent.filter(message => message.action === 'tab_group').at(-1)?.operation);
    await tab(1).click({ modifiers: ['Control'] });
    await tab(2).click({ modifiers: ['Control'] });
    assert.equal(await page.locator('.tab.group-selected').count(), 2);
    await tab(2).click({ button: 'right' });
    await page.locator('#group-create').click();
    await page.locator('#group-title').fill('Work');
    await page.locator('#group-color').selectOption('teal');
    await page.locator('#group-editor button[type=submit]').click();
    assert.deepEqual(await latest(), { kind: 'create', ids: [1, 2], title: 'Work', color: 'teal' });

    await page.evaluate(() => {
      window.savedSecond = document.querySelector('[data-tab-id="2"]');
      state.tabs[0].group = 8;state.tabs[1].group = 8;state.tabs[2].group = 9;
      window.browserUpdate({ ...state, tab_groups: [
        { id: 8, title: 'Work', color: 'teal', pinned: false, collapsed: false },
        { id: 9, title: '<fake-title>', color: 'red', pinned: false, collapsed: false },
      ] });
    });
    assert(await page.evaluate(() => savedSecond === document.querySelector('[data-tab-id="2"]')));
    assert.equal(await group(9).locator('.tab-group-name').textContent(), '<fake-title>');
    await group(9).click();
    assert.deepEqual(await latest(), { kind: 'collapse', id: 9 });
    await page.evaluate(() => {
      state.tab_groups[1].collapsed = true;
      window.browserUpdate({ ...state });
    });
    assert(!(await tab(3).isVisible()));
    assert.equal(await group(9).getAttribute('aria-expanded'), 'false');
    await tab(1).dragTo(group(9));
    assert.deepEqual(await latest(), { kind: 'assign', ids: [1], group: 9 });
    await group(8).dragTo(group(9), { targetPosition: { x: 3, y: 5 } });
    assert.deepEqual(await latest(), { kind: 'reorder', source: 8, target: 9, after: false });
    await tab(2).dragTo(page.locator('.tabs .new'));
    assert.deepEqual(await latest(), { kind: 'assign', ids: [2], group: null });
    await group(8).click({ button: 'right' });
    await page.locator('#group-pin').click();
    assert.deepEqual(await latest(), { kind: 'pin', id: 8 });
    await group(8).click({ button: 'right' });
    await page.locator('#group-dissolve').click();
    assert.deepEqual(await latest(), { kind: 'dissolve', id: 8 });

    for(const [width,theme] of [[690,0],[1000,2]]){
      await page.setViewportSize({width,height:600});
      await page.evaluate(theme=>window.browserUpdate({...state,appearance:{design:0,theme}}),theme);
      await group(8).click({button:'right'});
      const box=await page.locator('#tab-group-menu').boundingBox();
      assert(box.x>=0&&box.x+box.width<=width);
      if(process.env.LITERUST_GROUP_TEST_SCREENSHOTS){
        await page.screenshot({path:process.env.LITERUST_GROUP_TEST_SCREENSHOTS+'-'+width+'.png'});
      }
      await page.keyboard.press('Escape');
      assert(!(await page.locator('#tab-group-menu').isVisible()));
    }
    assert.deepEqual(errors, []);
    console.log('PASS: multi-select, group creation, collapse, pin, dissolve, group/tab drag and narrow menu');
  } finally {
    await browser.close();
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
