const {chromium}=require('playwright');
const {readFileSync}=require('node:fs');
const fixture=require('./rust-fixture.cjs');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({headless:true});
 try {
  const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
  for(const file of ['start','library']){
   const source=file==='start'?fixture('newtab-preview'):readFileSync(`ui/${file}.html`,'utf8');
   const html=source.replace('</head>',`<script>${readFileSync('ui/appearance.js','utf8')}\nwindow.applyAppearance({design:3,theme:2});</script></head>`);
   await page.setContent(html);
   assert.equal(await page.locator('html').getAttribute('data-theme'),'2');
   const light=await page.evaluate(()=>getComputedStyle(document.body).backgroundColor);
   await page.evaluate(()=>window.applyAppearance({design:4,theme:0}));
   assert.notEqual(await page.evaluate(()=>getComputedStyle(document.body).backgroundColor),light);
   assert.equal(await page.locator('html').getAttribute('data-design'),'4');
  }
  assert.deepEqual(errors,[]);console.log('PASS: embedded appearance script and live updates on start/library pages under CSP.');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
