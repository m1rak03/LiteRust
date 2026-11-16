const { chromium } = require('playwright');
const { execFileSync } = require('node:child_process');
const assert = require('node:assert/strict');
(async()=>{
 const html=execFileSync(require('node:path').join(require('node:os').homedir(),'.cargo','bin',process.platform==='win32'?'cargo.exe':'cargo'),['run','--quiet','--locked','--no-default-features','--example','security_controls'],{encoding:'utf8'});
 const browser=await chromium.launch({headless:true});
 try {
  const page=await browser.newPage();
  const violations=[];page.on('console',m=>{if(m.text().includes('Content Security Policy'))violations.push(m.text());});
  const attack='<script>window.compromised=1</script><img src="data:image/png,bad" onerror="window.compromised=2">';
  await page.setContent(html.replace('</body>',attack+'</body>'));
  assert.equal(await page.evaluate(()=>window.compromised),undefined);
  assert(violations.length>=2);
  assert.equal(await page.evaluate(()=>typeof window.browserUpdate),'function');
  await page.evaluate(()=>{window.sent=[];window.ipc={postMessage:v=>sent.push(JSON.parse(v))};});
  await page.locator('#address').fill('https://example.org');await page.locator('#address').press('Enter');
  assert.deepEqual(await page.evaluate(()=>sent.pop()),{action:'navigate',value:'https://example.org'});
  console.log('PASS: injected scripts/handlers blocked, trusted controls remain functional');
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
