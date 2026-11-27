const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{const b=await chromium.launch({headless:true});try{const p=await b.newPage({viewport:{width:1280,height:850},reducedMotion:'reduce'}),errors=[];p.on('pageerror',e=>errors.push(e.message));await p.setContent(fs.readFileSync('ui/toolbar.html','utf8').replace('/* APPEARANCE_SCRIPT */',fs.readFileSync('ui/appearance.js','utf8')));await p.evaluate(()=>{window.sent=[];window.ipc={postMessage:x=>sent.push(JSON.parse(x))};window.browserUpdate({tabs:[{id:1,title:'Новая вкладка'}],active:0,live_ids:[1],url:''});});
assert((await p.locator('#profile-account').boundingBox()).width<=106);
for(const action of ['minimize','maximize','close']){await p.locator('#window-'+action).click();assert.deepEqual(await p.evaluate(()=>sent.at(-1)),{action:'window_'+action});}assert.equal(await p.locator('#toggle-fullscreen').count(),0);await p.locator('#window-drag-space').dispatchEvent('mousedown',{button:0,detail:1});assert.deepEqual(await p.evaluate(()=>sent.at(-1)),{action:'window_drag'});await p.locator('#window-drag-space').dispatchEvent('mousedown',{button:0,detail:2});assert.deepEqual(await p.evaluate(()=>sent.at(-1)),{action:'window_maximize'});
await p.evaluate(()=>window.openPanel('settings'));await p.locator('[data-settings-category=network]').click();assert(await p.locator('#tls-help').isVisible());assert(!(await p.locator('#pref-font_size').isVisible()));await p.locator('#settings-search').fill('неттакогораздела');assert(await p.locator('#settings-empty').isVisible());await p.locator('[data-settings-category=all]').click();await p.locator('#settings-search').fill('');fs.mkdirSync('audit-logs/screenshots',{recursive:true});await p.screenshot({path:'audit-logs/screenshots/settings-local.png'});
await p.locator('[data-settings-category=network]').click();await p.evaluate(()=>window.openPanel('tools'));assert(await p.locator('#calc-a').isVisible());
await p.locator('#vpn-toolbar').click();
for(const width of [1280,690]){
 await p.setViewportSize({width,height:600});
 assert(await p.locator('#vpn').isVisible());
 assert(await p.locator('#vpn,.vpn-connection,.power-orbit').evaluateAll(els=>els.every(e=>e.scrollWidth<=e.clientWidth+1)));
}
await p.keyboard.press('Escape');assert(await p.locator('#vpn').isHidden());
await p.locator('#vpn-toolbar').click();await p.keyboard.press('Control+t');assert.deepEqual(await p.evaluate(()=>sent.at(-1)),{action:'shortcut',name:'new_tab'});
await p.setContent(fs.readFileSync('ui/dock.html','utf8').replace('/* APPEARANCE_SCRIPT */',fs.readFileSync('ui/appearance.js','utf8')));await p.evaluate(()=>{window.sent=[];window.ipc={postMessage:x=>sent.push(JSON.parse(x))};});for(const action of ['new_tab','duplicate_tab','print_page','new_private_window']){await p.locator(`[data-action=${action}]`).click();assert.deepEqual(await p.evaluate(()=>sent.at(-1)),{action});}
assert.deepEqual(errors,[]);console.log('PASS: compact account, window controls and drag, settings categories/search, isolated tools, Ctrl+T from VPN, dock actions');}finally{await b.close();}})().catch(e=>{console.error(e);process.exit(1)});
