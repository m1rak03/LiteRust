# 0.13.0-alpha.1 validation notes

Features: in-place upgrade with graceful browser close; independent new-tab design; background file/HTTPS URL; stable animated tabs and motion toggle; compact tabs, clock/greeting visibility and background dimming; built-in SVG icons and custom PNG icons for actions, shortcuts and Windows app identity.

VPN: browser-only Xray HTTP proxy or additional elevated sing-box 1.14.4 TUN helper routing Windows through Xray SOCKS. The helper is owned through a Windows kill-on-close job and parent control connection. Closing LiteRust/disconnecting restores ordinary Windows networking; this is not a persistent system kill switch. UDP support depends on the upstream node. Transport hostnames/SNI are preserved when pinning the VPN endpoint before TUN startup. Private windows do not create a second system tunnel.

Source for the separately distributed unmodified sing-box core and license are in vendor/sing-box/SOURCE.txt. Binary download SHA-256 is verified by the release workflow.

Local validation: Rust tests, Windows-target Clippy, Chromium UI tests. Windows release gate additionally validates native TUN-to-SOCKS TCP routing on reserved benchmark addresses only, cleanup, startup, upgrade from running 0.12 preserving settings, and uninstall. This does not prove connectivity with every VPN subscription, UDP service or Windows configuration.

Tor bridges remain unfinished and are NOT included. Releases do not self-download in the background; users run the new installer to upgrade.
