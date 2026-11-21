// Local-only integration: Chromium -> Xray -> mock upstream HTTP proxy.
const {chromium}=require('playwright');const net=require('node:net');const fs=require('node:fs');const {spawn}=require('node:child_process');const assert=require('node:assert/strict');
const path=require('node:path');const os=require('node:os');const fixture=require('./rust-fixture.cjs');
(async()=>{
 const root=fs.realpathSync(os.tmpdir());
 const directory=fs.mkdtempSync(path.join(root,'literust-xray-fixture-'));
 let requests=[];let sockets=new Set();let core,browser;
 const upstream=net.createServer(socket=>{
  sockets.add(socket);socket.on('error',()=>{});socket.on('close',()=>sockets.delete(socket));let buffer='';let tunneled=false;
  socket.on('data',chunk=>{buffer+=chunk.toString();let end=buffer.indexOf('\r\n\r\n');if(end<0)return;
   const header=buffer.slice(0,end);buffer=buffer.slice(end+4);const first=header.split('\r\n')[0];
   if(first.startsWith('CONNECT ')&&!tunneled){requests.push(first);tunneled=true;socket.write('HTTP/1.1 200 Connection established\r\n\r\n');return;}
   requests.push(first);const body='<title>VPN routing test</title><h1>VPN path verified</h1>';
   socket.end(`HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: ${Buffer.byteLength(body)}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n${body}`);
  });
 });
 try {
  await new Promise((resolve,reject)=>{upstream.once('error',reject);upstream.listen(19181,'127.0.0.1',resolve);});
  fixture('vpn-fixtures',directory);
  const binary=process.env.LITERUST_XRAY||(process.platform==='win32'
    ?path.resolve(__dirname,'../vendor/xray/xray.exe'):'xray');
  core=spawn(binary,['run','-config','stdin:','-format','json'],{stdio:['pipe','ignore','ignore'],windowsHide:true});
  let launchError;core.on('error',error=>{launchError=error;});
  core.stdin.on('error',error=>{launchError=error;});core.stdin.end(fs.readFileSync(path.join(directory,'integration.json')));
  await new Promise((resolve,reject)=>{let tries=0;const attempt=()=>{if(launchError){reject(launchError);return;}const s=net.connect(19180,'127.0.0.1');s.once('connect',()=>{s.destroy();resolve();});s.once('error',()=>{s.destroy();if(++tries>50)reject(new Error('Xray did not start'));else setTimeout(attempt,100);});};attempt();});
  browser=await chromium.launch({headless:true,args:['--proxy-server=http://127.0.0.1:19180','--proxy-bypass-list=<-loopback>','--disable-quic','--force-webrtc-ip-handling-policy=disable_non_proxied_udp']});
  const page=await browser.newPage();await page.goto('http://vpn-check.invalid/first',{timeout:15000});assert.equal(await page.locator('h1').textContent(),'VPN path verified');assert(requests.some(r=>r.startsWith('CONNECT vpn-check.invalid:80')));
  await new Promise(resolve=>{core.once('exit',resolve);core.kill();});core=null;
  let failed=false;try{await page.goto('http://vpn-check.invalid/after-stop',{timeout:10000});}catch(e){failed=/ERR_PROXY_CONNECTION_FAILED|ERR_TUNNEL_CONNECTION_FAILED/.test(e.message);}
  assert(failed,'Browser must fail through its unavailable proxy, not switch to direct');
  console.log('PASS: Chromium HTTP traffic traverses Xray and the configured upstream proxy; stopping Xray causes ERR_PROXY_CONNECTION_FAILED/ERR_TUNNEL_CONNECTION_FAILED. No remote provider or real key used.');
 } finally {if(browser)await browser.close();if(core)core.kill();for(const s of sockets)s.destroy();await new Promise(resolve=>upstream.close(resolve));
  if(path.dirname(fs.realpathSync(directory))===root)fs.rmSync(directory,{recursive:true,force:true});
 }
})().catch(e=>{console.error(e);process.exitCode=1;});
