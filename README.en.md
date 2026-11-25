# LiteRust

Settings open in their own tab. Appearance supports separate background and accent colors. Google Password Manager and Yandex ID security open in LiteRust. After installation, Windows Default Apps can associate LiteRust with HTTP and HTTPS.


[English](README.en.md) · [Русский](README.md)

LiteRust is a Windows browser built with Rust and Microsoft WebView2. Arrange quick links and widgets on the start page, keep work and personal browsing in separate profiles, and open your bookmarks, history and downloads from the sidebar.

## Using LiteRust

- **Start page.** Drag quick-site tiles to reorder them. Use Shift+Left or Shift+Right on a focused tile to move it with the keyboard. Use Customize page to rearrange widgets and save the layout.
- **Sidebar.** Hover over the collapsed left sidebar to reveal its labels. You can also use full expansion on hover or pin the sidebar in Settings → Appearance.
- **Search.** The address bar and start page suggest sites from your local bookmarks and history. Private windows omit these suggestions. LiteRust passes your light or dark preference to WebView2. Providers with their own theme settings may override it. DuckDuckGo also receives its theme in the search URL.
- **Printing.** Open Tools → Print to preview the page or save it as a PDF with a custom header and footer. Print the exported PDF to use those captions on paper.
- **Language.** Choose English or Russian in Settings → Startup and tabs, save your change and restart LiteRust.
- **Accounts.** Open the official sign-in pages for Google, Yandex, Microsoft, Yahoo, Opera or Mozilla (Firefox). Site sessions belong to the active local profile. Opera Sync and Firefox Sync continue to run in their respective browsers.
- **Windows.** Ctrl+N opens a regular window and Next window cycles through the active profile’s windows. Restore startup reopens saved windows and tabs.
- **Split view.** Open two tabs, then right-click one and choose Open beside. Each pane has an address bar and independent scrolling. Use the split toolbar to select tabs, choose a layout, resize or swap the panes. See the [Split View notes](docs/releases/0.17.0-alpha.2.md).

Workspaces keep separate sets of tabs, groups, and quick links in the same profile. Open the Workspaces button beside your profile to switch. For concurrent work, open another window and select a different workspace. [Workspaces](docs/releases/0.17.0-alpha.3.md)

Find a tab, bookmark, quick link, or visited page in the address bar. Type “>” to search browser commands. [Address bar](docs/releases/0.17.0-alpha.4.md)

Use Tools to keep notes, customize shortcuts, open selected text in Google Translate, or save the current page through WebView2. [Browser tools and notes](docs/releases/0.17.0-alpha.5.md)

In Settings → Performance, choose how to manage tab memory or measure WebView2 resource use. [Tab resources](docs/releases/0.18.0-alpha.1.md) · [Background activity](docs/releases/0.18.0-alpha.2.md)

Startup timings are available in the local log. [Startup phases and measurements](docs/releases/0.18.0-alpha.3.md)

Settings → Diagnostics shows component status and lets you review and save a local report. [Diagnostics](docs/releases/0.18.0-alpha.4.md)

[Interface checks and comparison across 0.16–0.18](docs/releases/0.18.0-alpha.5.md)

- **Tab groups.** Right-click a tab to create a group, or Ctrl-click several tabs first to group the selection. Click a group header to collapse it. Right-click the header to edit, pin or ungroup. See the [0.17.0-alpha.1 release notes](docs/releases/0.17.0-alpha.1.md).
- **Downloads and bookmarks.** Filter downloads, check progress and open the destination folder. WebView2 controls the availability of pause, resume and cancel. Search and sort bookmarks, and create folders to organise them. See the [0.16.0-alpha.5 release notes](docs/releases/0.16.0-alpha.5.md).
- **Memory.** Background tabs use WebView2’s low-memory mode. Optional tab discard releases inactive views when the tab budget is exceeded. Discarded tabs reload when selected, so unsaved form state can be lost.

Profile settings also show storage sizes and let you restore or delete archived profiles and back up settings and tabs locally. See the [0.16.0-alpha.3 release notes](docs/releases/0.16.0-alpha.3.md).

## Certificates and network

Settings → Network and certificates links to the [Gosuslugi instructions](https://www.gosuslugi.ru/tls), the [Russian National Certification Authority](https://nuc.voskhod.ru/) and the Windows certificate manager. Follow the official instructions to install the root and issuing certificates if you need them. Installing a root certificate expands the trust of Windows applications. LiteRust relies on WebView2 for TLS validation.

The VPN hub can route browser traffic through Xray. The optional Windows-wide mode uses sing-box and requires administrator permission. See the [Russian settings reference](docs/SETTINGS.md) for supported configurations.

## Install or build

Download the Windows x64 installer from [GitHub Releases](https://github.com/m1rak03/LiteRust/releases). LiteRust requires the Microsoft Edge WebView2 Runtime. The full installer includes runtime setup.

For development on Windows, use the Rust MSVC toolchain, Visual Studio Build Tools and Windows SDK.

```powershell
cargo check --locked --all-targets
cargo test --locked
cargo build --locked --release
node tools/check-source.mjs
```

On other platforms, run the platform-independent tests with `cargo test --locked --no-default-features --lib`.

Report problems through [GitHub Issues](https://github.com/m1rak03/LiteRust/issues/new/choose). Include your LiteRust version and steps to reproduce. For security reports, see [SECURITY.md](SECURITY.md).
