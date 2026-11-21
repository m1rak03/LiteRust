const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

async function draw(browser, root) {
  const page = await browser.newPage({ viewport: { width: 690, height: 720 }, reducedMotion: 'reduce', deviceScaleFactor: 1.25 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const layout = fs.readFileSync(path.join(root, 'ui/toolbar.html'), 'utf8')
    .replace('/* APPEARANCE_SCRIPT */', fs.readFileSync(path.join(root, 'ui/appearance.js'), 'utf8'));
  await page.setContent(layout);
  const metrics = await page.evaluate(() => {
    window.sent = [];
    window.ipc = { postMessage: json => sent.push(JSON.parse(json)) };
    const longTitle = 'A long title that should stay within its tab '.repeat(10);
    window.fixture = {
      tabs: Array.from({length:24}, (_, i) => ({ id:i+1, title:'Tab '+i })),
      active:0, live_ids:[1], url:'https://example.org/', appearance:{design:13, theme:11}, window_count:1,
      local_profiles:{profiles:[{id:'main',name:'Main'},{id:'work',name:'Work '+longTitle}]},
      current_profile:'main',
      profile_details:{bytes:{main:1000,work:2000},archived:[]},
    };
    window.browserUpdate(fixture);
    window.openPanel('tools');
    const tools=document.getElementById('tools-tabs');
    const icons=document.getElementById('back').firstElementChild;
    const toolsRow=tools.firstElementChild;
    const icon=icons.firstElementChild;
    const toolbarObserver=new MutationObserver(()=>{});
    toolbarObserver.observe(icons,{childList:true,subtree:true});
    const toolsObserver=new MutationObserver(()=>{});
    toolsObserver.observe(tools,{childList:true,subtree:true});
    for(let i=0;i<200;i++){
      const tabs=fixture.tabs.map((tab,j)=>j===23?{ ...tab, title:'Changing '+i }:tab);
      window.browserUpdate({...fixture,tabs,loading:i%2===0});
    }
    const counts={
      toolbarReplacements:toolbarObserver.takeRecords().length,
      toolListMutations:toolsObserver.takeRecords().length,
      steadyIcon:icon===icons.firstElementChild,
      steadyToolRow:toolsRow===tools.firstElementChild,
      narrow:document.documentElement.scrollWidth<=innerWidth,
    };
    toolbarObserver.disconnect();
    toolsObserver.disconnect();
    window.openPanel('bookmarks-manager');
    const bookmarks=[
      {url:'https://example.org/',title:'Original',added_at:1},
      {url:'https://example.net/',title:'Second',added_at:2},
    ];
    window.bookmarkManagerUpdate({items:bookmarks,folders:[]});
    const unchanged=document.querySelector('#bookmark-list [data-row-key="https://example.net/"]');
    window.bookmarkManagerUpdate({items:[{...bookmarks[0],title:'Revised'},bookmarks[1]],folders:[]});
    counts.steadyBookmark=!!unchanged&&unchanged===document.querySelector('#bookmark-list [data-row-key="https://example.net/"]');
    const profiles=document.getElementById('local-profile-list');
    const profile=profiles.firstElementChild;
    window.browserUpdate({...fixture,profile_details:{bytes:{main:2000,work:2000},archived:[]}});
    counts.steadyProfile=profile===profiles.firstElementChild;
    return counts;
  });
  assert.deepEqual(errors, []);
  await page.close();
  return metrics;
}

(async () => {
  const browser = await chromium.launch({headless:true});
  try {
    const args=process.argv.slice(2);
    const current=await draw(browser,'.');
    assert.equal(current.toolbarReplacements,0);
    assert.equal(current.toolListMutations,200);
    assert(current.steadyIcon&&current.steadyToolRow&&current.steadyBookmark&&current.steadyProfile&&current.narrow);
    if(args.length)console.log('baseline '+JSON.stringify(await draw(browser,args[0])));
    console.log('current '+JSON.stringify(current));
    console.log('PASS: changing one tab preserves toolbar icons, unaffected rows and narrow layout at 125% scale');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode=1; });
