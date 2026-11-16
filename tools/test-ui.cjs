// Tests local UI in Chromium, not the Windows/WebView2 application.
const { chromium } = require('playwright');
const { readFileSync, mkdirSync } = require('node:fs');
const { strict: assert } = require('node:assert');
(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({viewport:{width:1280,height:800}});
    const errors=[]; page.on('pageerror', e=>errors.push(e.message));
    await page.addInitScript(()=>{});
    await page.setContent(readFileSync('ui/toolbar.html','utf8').replace('/* APPEARANCE_SCRIPT */',readFileSync('ui/appearance.js','utf8')));
    await page.evaluate(()=>{window.sent=[]; window.ipc={postMessage: v=>sent.push(JSON.parse(v))};});
    const state={live_ids:[1],private_mode:false,cosmetic_filter:false,network_blocking:false,site_host:'example.org',site_adblock_exception:false,live_tab_budget:3,auto_discard:false,url_warning:null,tabs:[{id:1,title:'<img src=x onerror=alert(1)>',url:'https://example.org/'}],active:0,url:'https://example.org/',loading:false,bookmarked:false,bookmarks:0,history:0,downloads:0,can_back:false,can_forward:false};
    await page.evaluate(s=>window.browserUpdate(s),state);
    assert.equal(await page.locator('.tab-title').textContent(), state.tabs[0].title);
    assert.equal(await page.locator('.tab-title img').count(),0);
    await page.locator('#address').fill('пример.рф');
    await page.locator('#address').press('Enter');
    assert.deepEqual(await page.evaluate(()=>sent.pop()), {action:'navigate', value:'пример.рф'});
    await page.locator('#more').click();
    page.once('dialog',d=>d.dismiss());
    await page.locator('#toggle-auto-discard').click();
    assert.equal(await page.evaluate(()=>sent.length),0);
    await page.locator('#more').click();
    page.once('dialog',d=>d.accept());
    await page.locator('#toggle-auto-discard').click();
    assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'toggle_auto_discard'});
    await page.setViewportSize({width:690,height:400});
    await page.locator('#more').click();
    assert(await page.locator('#toggle-auto-discard').isVisible());
    mkdirSync('audit-logs/screenshots',{recursive:true});
    await page.screenshot({path:'audit-logs/screenshots/toolbar-690.png'});
    await page.locator('#show-appearance').click();
    assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'appearance_panel',open:true});
    assert.equal(await page.locator('.design-choice').count(),30);
    assert.equal(await page.locator('.theme-choice').count(),330);
    assert.equal(await page.evaluate(()=>{let n=0;for(let design=0;design<30;design++)for(let theme=0;theme<330;theme++){window.applyAppearance({design,theme});if(document.documentElement.dataset.theme!==String(theme))throw Error('theme truncated');n++;}return n;}),9900);
    for(let design=0;design<30;design++){
      await page.locator(`.design-choice[data-design="${design}"]`).click();
      for(const theme of [0,2,54,255,329]){
        await page.locator(`.theme-choice[data-theme="${theme}"]`).click();
        assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'set_appearance',design,theme});
        assert.equal(await page.locator('html').getAttribute('data-theme'),String(theme));
        assert.equal(await page.locator('.theme-choice[aria-pressed=true]').count(),1);
      }
    }
    await page.locator('#theme-filter').selectOption('light');
    assert.equal(await page.locator('.theme-choice:visible').count(),132);
    await page.locator('#theme-filter').selectOption('contrast');
    assert.equal(await page.locator('.theme-choice:visible').count(),66);
    await page.locator('#theme-filter').selectOption('all');
    await page.evaluate(s=>window.browserUpdate({...s,tabs:[{id:1,title:'Новая вкладка'}],url:''}),state);
    await page.setViewportSize({width:1120,height:620});
    await page.locator('.design-choice[data-design="0"]').click();
    await page.locator('.theme-choice[data-theme="0"]').click();
    await page.screenshot({path:'audit-logs/screenshots/appearance-dark.png'});
    await page.locator('.design-choice[data-design="2"]').click();
    await page.locator('.theme-choice[data-theme="7"]').click();
    await page.screenshot({path:'audit-logs/screenshots/appearance-light.png'});
    await page.locator('#close-appearance').click();
    assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'appearance_panel',open:false});
    await page.evaluate(s=>window.browserUpdate(s),state);
    // Loading/count changes must not rebuild tab DOM or lose keyboard focus.
    assert(await page.evaluate(s=>{const tab=document.querySelector('.tab');window.browserUpdate({...s,loading:true});return tab===document.querySelector('.tab');},state));
    await page.evaluate(()=>{sent=[];});
    await page.locator('#more').click();
    await page.locator('#google-account').click();
    assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'open_google_account'});
    await page.locator('#more').click();
    page.once('dialog',d=>d.dismiss());
    await page.locator('#discard-background').click();
    assert.equal(await page.evaluate(()=>sent.length),0);
    await page.locator('#more').click();
    page.once('dialog',d=>d.accept());
    await page.locator('#discard-background').click();
    assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'discard_background'});
    const html=readFileSync('ui/library.html' ,'utf8').replace('__TITLE__','Закладки').replace('<!-- ENTRIES -->','<a class="entry" href="https://example.org">Alpha</a><a class="entry" href="https://example.com">Бета</a>');
    await page.setContent(html);
    await page.locator('#filter').fill('БЕТА');
    assert.equal(await page.locator('.entry:visible').count(),1);
    assert.equal(await page.locator('.entry:visible').textContent(),'Бета');
    await page.locator('#filter').fill('');
    assert.equal(await page.locator('.entry:visible').count(),2);
    await page.screenshot({path:'audit-logs/screenshots/library.png'});
    assert.deepEqual(errors,[]);
    console.log('PASS: Chromium UI — escaped titles, address IPC, discard accept/cancel, 690px menu, 9900 renderer combinations and 150 picker combinations, theme filters, stable tab DOM, Google account command, Cyrillic library search; no page errors.');
    console.log('Chromium '+browser.version());
  } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exitCode=1});
