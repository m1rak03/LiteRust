// Dependency-free static checks. These are not a Rust compile or Windows run.
import { readFileSync } from 'node:fs';
import { Script } from 'node:vm';
import { strict as assert } from 'node:assert';

for (const page of ['toolbar', 'start', 'library', 'dock']) {
  const html = readFileSync(new URL(`../ui/${page}.html`, import.meta.url), 'utf8');
  for (const [, src] of html.matchAll(/<script>([\s\S]*?)<\/script>/g)) {
    new Script(src, { filename: page + '.js' });
  }
}
new Script(readFileSync(new URL('../ui/appearance.js', import.meta.url), 'utf8'), {filename:'appearance.js'});
new Script(readFileSync(new URL('../ui/cosmetic-filter.js', import.meta.url), 'utf8'), {filename:'cosmetic-filter.js'});
new Script(readFileSync(new URL('../ui/page-activity.js', import.meta.url), 'utf8'), {filename:'page-activity.js'});
const ui = readFileSync(new URL('../ui/toolbar.html', import.meta.url), 'utf8');
const main = readFileSync(new URL('../src/main.rs', import.meta.url), 'utf8');
const storage = readFileSync(new URL('../src/storage.rs', import.meta.url), 'utf8');
const portal = readFileSync(new URL('../src/google_passwords.rs', import.meta.url), 'utf8');
for(const id of ['google-passwords', 'new-private-window', 'toggle-cosmetic-filter', 'toggle-network-blocking', 'toggle-site-adblock', 'cycle-memory-mode', 'mode-badge', 'export-bookmarks', 'clear-download-history']) {
  assert.match(ui, new RegExp(`id="${id}"`));
}
for(const action of ['open_google_passwords','new_private_window','toggle_cosmetic_filter','toggle_network_blocking','toggle_site_adblock','cycle_memory_mode', 'export_bookmarks', 'clear_download_history']) {
  assert.match(ui, new RegExp(`send\\('${action}'\\)`));
}
assert.match(main, /with_incognito\(true\)/);
assert.match(main, /with_permission_handler\(site_permission_policy\)/);
assert.match(main, /update_background_hints/);
assert.match(main, /network_block::apply/);
assert.match(main, /update_filter_for_live_pages/);
assert.match(storage, /adblock_exceptions: Vec<String>/);
assert.match(storage, /toggle_adblock_exception/);
assert.match(storage, /live_tab_budget: u8/);
assert.match(storage, /bookmarks_as_html/);
assert.match(storage, /json\.bak/);
assert.match(storage, /MAX_BROWSER_DATA_BYTES/);
assert.match(main, /enforce_page_budget/);
const block = readFileSync(new URL('../src/network_block.rs', import.meta.url), 'utf8');
const sec = readFileSync(new URL('../src/security.rs', import.meta.url), 'utf8');
assert.match(block, /Network\.setBlockedURLs/);
assert.match(block, /Network\.enable/);
assert.match(sec, /url\.username\(\)\.is_empty\(\)/);
const resources = readFileSync(new URL('../src/browser_resources.rs', import.meta.url), 'utf8');
assert.match(resources, /self\.data\.live_tab_budget/);
assert.match(resources, /MemoryUsageLevel::Low/);
assert.doesNotMatch(ui, /downloadPoll/);
assert.match(resources, /page\.navigation_serial == pending\.navigation/);
assert.match(main, /pages: HashMap<u64, LivePage>/);
assert.match(main, /let \(mut data, data_file\) = if private_mode/);
assert.match(main, /if !loading && !self\.private_mode/);
assert.match(storage, /pub id: u64/);
assert.match(portal, /https:\/\/passwords\.google\.com\//);
assert.doesNotMatch(portal, /chrome:.*login.*data/i);
// Never expose the page WebView to the privileged toolbar IPC transport.
assert.equal((main.match(/\.with_ipc_handler\(/g)||[]).length, 2);

// A missing Runtime must be detected before the native window is created.
const runBody = main.slice(main.indexOf('fn run() ->'));
assert(runBody.indexOf('wry::webview_version()') < runBody.indexOf('WindowBuilder::new()'));
assert.match(runBody, /version\.trim\(\)\.is_empty\(\)/);
console.log('LiteRust static source checks: OK');
