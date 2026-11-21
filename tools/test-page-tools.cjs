const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');

(async () => {
  const browser = await chromium.launch({ headless:true });
  try {
    const page = await browser.newPage({ viewport:{width:690,height:620} });
    const errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    await page.setContent(fs.readFileSync('ui/toolbar.html','utf8')
      .replace('/* APPEARANCE_SCRIPT */',fs.readFileSync('ui/appearance.js','utf8')));
    await page.evaluate(()=>{
      window.sent=[];
      window.ipc={postMessage: json=>sent.push(JSON.parse(json))};
      window.toolFixture={
        tabs:[{id:25,title:'Fixture'}],active:0,live_ids:[25],url:'https://example.org/',
        hidden_tools:[],window_count:1
      };
      window.browserUpdate(toolFixture);
      window.openPanel('tools');
      window.notesUpdate({reset:true,changes:[{id:1,title:'<b>First</b>',body:'Letter',edited_at:1},
        {id:2,title:'Second',body:'Meeting at noon',edited_at:2}],removed:[],select:null});
    });
    const last=action=>page.evaluate(action=>sent.filter(message=>message.action===action).at(-1),action);
    await page.locator('#notes-search').fill('noon');
    assert.equal(await page.locator('#notes-list button').count(),1);
    await page.locator('#notes-list button').click();
    assert.equal(await page.locator('#tool-notes').inputValue(),'Meeting at noon');
    await page.locator('#notes-title').fill('New title');
    assert.deepEqual(await last('note_edit'),{action:'note_edit',id:2,edit:{kind:'title',value:'New title'}});
    await page.locator('#tool-notes').fill('Draft text');
    assert.deepEqual(await last('note_edit'),{action:'note_edit',id:2,edit:{kind:'body',value:'Draft text'}});
    await page.locator('#notes-link').click();
    assert.deepEqual(await last('note_link'),{action:'note_link',id:2,attach:true});
    await page.locator('#notes-search').fill('');
    assert.equal(await page.locator('[data-note-id="1"] b').count(),0);
    page.once('dialog', dialog=>dialog.accept());
    await page.locator('#notes-remove').click();
    assert.deepEqual(await last('note_remove'),{action:'note_remove',id:2});
    await page.locator('#notes-create').click();
    assert.deepEqual(await last('note_create'),{action:'note_create'});

    await page.locator('#tool-preferences summary').click();
    await page.locator('#tool-preferences input[value=calculator]').uncheck();
    assert.deepEqual(await last('set_hidden_tools'),{action:'set_hidden_tools',tools:['calculator']});
    await page.evaluate(()=>window.browserUpdate({...toolFixture,hidden_tools:['calculator']}));
    assert.equal(await page.locator('#calc-a').isVisible(),false);

    await page.locator('#tool-translate').click();
    assert(await last('translate_selection'));
    await page.evaluate(()=>window.translationPreview('<b>fixture</b>'));
    assert.equal(await page.locator('#translation-text').textContent(),'<b>fixture</b>');
    assert.equal(await page.locator('#translation-text b').count(),0);
    assert.equal(await last('translate_open'),undefined);
    await page.locator('#translation-language').selectOption('en');
    await page.locator('#translation-open').click();
    assert.deepEqual(await last('translate_open'),{action:'translate_open',language:'en'});

    await page.evaluate(()=>window.openPanel('tools'));
    await page.locator('#tool-find').click();
    assert.deepEqual(await last('shortcut'),{action:'shortcut',name:'find'});
    await page.evaluate(()=>window.openPanel('tools'));
    await page.locator('#tool-save-page').click();
    assert.deepEqual(await last('save_page'),{action:'save_page'});
    assert.deepEqual(errors,[]);
    console.log('PASS: note search, edits, linking, deletion, tool settings, explicit translation and Save As commands');
  } finally {
    await browser.close();
  }
})().catch(error=>{ console.error(error);process.exitCode=1; });
