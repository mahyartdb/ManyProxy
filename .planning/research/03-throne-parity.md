# Research 03 — Throne feature surface (primary: Throne repo, docs, routeprofiles)

**Decision this informs:** v1 parity scope vs explicitly deferred surface.
**Confidence:** high on listed items; gaps flagged.

## Catalog (claim — source)

- Qt + sing-box + Xray-core + Extra Core; Win 11/10/8/7, Linux, macOS — README.
- Dist: Win x64/x86/ARM64 installer vs portable; Linux glibc-portables/debs; macOS arm64/x64/legacy; managers (winget/scoop/dnf/zypper/AUR/Nix/RPM); source build `clone --recursive + srslist.h + cmake` — downloads + installation docs + 1.3.0 notes.
- macOS quarantine/setuid-root, Linux `/opt/Throne` installer + `~/.config/Throne`, self-healing `throne://` registration — installation + deeplinks docs.
- Protocols: SOCKS/HTTP(S)/SS/Trojan/VMess/VLESS/TUIC/Hysteria/1/2/AnyTLS/Mieru/Snell/Naïve/Juicity/TrustTunnel/ShadowTLS/WireGuard/AmneziaWG/MASQUE/SSH/Xray-VLESS/OpenVPN/Custom- polit-Chaining/ExtraCore — README + 1.2.x/1.3.x deltas.
- Modes: System Proxy vs TUN mutually exclusive; start node first; force-quit proxy-stuck fix; Win TUN hardening (DisableSmartNameResolution + disable QUIC) — configuration + windows_tun_mode docs.
- TUN settings fragments only (strict-route default-off, auto_redirect, exclude/alter private ranges, L3 bridge, endpoints-in-profiles, loop/DNS fixes) — release notes; full dialog undocumented.
- Routing presets repo `routeprofiles` (Bypass_China/Iran/Russia, Proxy_Antizapret/Refilter/Blocked...) in `.json` rule arrays + extensionless `throne://route` links; local normal vs Raw; defaults `proxy/direct/block/warp-bypass`; remote auto-update interval; `throne://route|remoteroute` grammars; connections→route add — configuration docs + routeprofiles + deeplinks + releases.
- DNS: Google DoH default; System DNS AV-flagged; macOS TUN DNS fix; Xray DNS/loopback fixes — releases + README.
- Subs/import: share links, sing-box JSON variants, full Xray JSON array, v2rayN, limited SS/Clash; `Ctrl+V`/drag-drop/launch-arg/single-instance; `throne://addsub|add`; multi-file import — README + configuration + deeplinks + releases.
- Hotkeys verified: `Ctrl+V/Ctrl+U/Ctrl+Shift+S/Enter/Ctrl+C/Ctrl+Alt+C`; toolbar start/stop refactor; runtime stats; tray widgets + macOS fix — docs + releases. Full map undocumented.
- Per-config: URL test + lowest-latency enable; share/deep-link/QR; connections per-process tree + close + add-to-route; 90-day traffic stats; security view + remove-insecure; OTP; auto-selector + failover; diagnostics profiler; custom JSON editor — releases + docs.

## Gaps (Nekoray-inherited, verify against build before spec-ing)

Transports/uTLS/Reality/chain editor fields; inbound host:port/auth editors; TUN dialog toggles; Basic/Advanced rule matchers + ruleset compile flow; DNS editors + hijack toggle UI; group types/UA/expiry/filters; Clash/v2rayN edge cases; TCP-ping vs URL-test semantics, sort/speed-test settings, latency units.
