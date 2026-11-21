const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 690, height: 620 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.setContent(fs.readFileSync('ui/toolbar.html', 'utf8')
      .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync('ui/appearance.js', 'utf8')));
    await page.evaluate(() => {
      window.sent=[];
      window.ipc={postMessage: value=>sent.push(JSON.parse(value))};
      window.omniboxFixture={
        tabs:[{id:4,title:'Current'},{id:9,title:'Research'}], active:0,
        live_ids:[4,9], url:'https://current.example/', current_workspace:2,
        search_engine:'bing', window_count:1
      };
      window.browserUpdate(omniboxFixture);
    });
    const last = action => page.evaluate(action => sent.filter(message=>message.action===action).at(-1), action);
    async function request(input, text) {
      const previous = (await last('suggest'))?.request;
      await input.fill(text);
      await page.waitForFunction(({text,previous})=>sent.some(message=>
        message.action==='suggest'&&message.query===text&&message.request!==previous),{text,previous});
      return last('suggest');
    }
    async function respond(request, items) {
      await page.evaluate(({request,items})=>window.showSuggestions(request.query,items,request.request),{request,items});
    }
    const input=page.locator('#address');
    const options=page.locator('#address-suggestions [role=option]');
    assert.equal(await page.locator('.address-wrap [data-search-provider]').textContent(),'Bing');
    const data = [
      {title:'<b>Research</b>', detail:'https://research.example/', label:'Tab',
        completion:'https://research.example/', action:{kind:'tab',id:9}},
      {title:'research', detail:'Bing', label:'Search', completion:'research',
        action:{kind:'search',query:'research'}},
    ];
    const initial = await request(input,'research');
    assert.equal(initial.workspace,2);
    await respond(initial,data);
    assert.equal(await options.count(),2);
    assert.equal(await options.first().locator('b').count(),0);
    assert((await options.first().textContent()).includes('<b>Research</b>'));
    await input.press('ArrowDown');
    assert.equal(await options.first().getAttribute('aria-selected'),'true');
    await input.press('ArrowUp');
    assert.equal(await options.last().getAttribute('aria-selected'),'true');
    await input.press('ArrowDown');
    await input.press('Tab');
    assert.equal(await input.inputValue(),'https://research.example/');
    assert.equal(await input.evaluate(el=>el===document.activeElement),true);
    await input.press('Enter');
    assert.deepEqual(await last('omnibox_pick'), {action:'omnibox_pick',target:{kind:'tab',id:9},workspace:2,pane:null});

    const fresh = await request(input,'research');
    await respond(initial,data);
    assert.equal(await options.count(),0,'Discard stale suggestion reply with same text');
    await respond(fresh,data);
    await input.press('Escape');
    assert.equal(await options.count(),0);
    assert.equal(await input.inputValue(),'research');
    await input.press('Escape');
    assert.equal(await input.inputValue(),'https://current.example/');

    await input.fill('> settings');
    await input.press('Enter');
    const command = await last('suggest');
    await respond(command,[{title:'Open settings',detail:'',label:'Command',
      completion:'> Open settings',action:{kind:'command',command:'settings'}}]);
    assert.deepEqual((await last('omnibox_pick')).target,{kind:'command',command:'settings'});

    await page.evaluate(() => window.browserUpdate({...omniboxFixture,search_engine:'duckduckgo',
      split:{panes:[4,9],focus:'first',axis:'horizontal',percent:50,
        urls:['https://current.example/','https://research.example/']}}));
    assert.equal(await page.locator('.address-wrap [data-search-provider]').textContent(),'DuckDuckGo');
    const secondary=page.locator('form[data-pane=second] input');
    const splitRequest = await request(secondary,'notes');
    await respond(splitRequest,[{title:'notes',detail:'DuckDuckGo',label:'Search',
      completion:'notes',action:{kind:'search',query:'notes'}}]);
    assert.equal(await page.locator('form[data-pane=second] [role=listbox]').count(),1);
    await secondary.press('ArrowDown');
    await secondary.press('Enter');
    assert.deepEqual(await last('omnibox_pick'), {action:'omnibox_pick',
      target:{kind:'search',query:'notes'},workspace:2,pane:'second'});
    assert.deepEqual(errors,[]);
    console.log('PASS: local address-bar rows, keyboard, commands, stale replies, search provider and Split View');
  } finally {
    await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
