#!/usr/bin/env python3
"""Round 3: A-light bench, loopback IP:port + Start + Edit always-on. Detailed iterations."""
import os
OUT = r"E:\Code\ManyProxy\.impeccable\mocks\decision\round3"
os.makedirs(OUT, exist_ok=True)

SANS = "Segoe UI, system-ui, sans-serif"
MONO = "Consolas, Cascadia Mono, monospace"
PAPER, QUIET, HAIR, INK, MUT = "#ffffff", "#f5f5f5", "#e5e5e5", "#0a0a0a", "#737373"
CHROME, PRI, PRI_T, GO, STOP, RIM = "#F8F8F8", "#171717", "#fafafa", "#22c55e", "#ef4444", "#60a5fa"
WASH = "#dbeafe"

def esc(s): return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def chrome_top(title, W):
    return (f'<rect x="0" y="0" width="{W}" height="30" fill="{CHROME}" stroke="{HAIR}" stroke-width="1"/>'
        f'<text x="12" y="20" fill="{INK}" font-size="13" font-weight="600">ManyProxy</text>'
        f'<text x="{W-220}" y="20" fill="{MUT}" font-size="11">local-only · no system proxy · no TUN</text>'
        f'<rect x="0" y="30" width="{W}" height="34" fill="{PAPER}" stroke="{HAIR}" stroke-width="1"/>'
        f'<text x="12" y="52" fill="{INK}" font-size="12">Program  Settings  Groups  Routing  Tools</text>'
        f'<rect x="{W-158}" y="35" width="146" height="24" rx="6" fill="{PRI}"/><text x="{W-85}" y="51" text-anchor="middle" fill="{PRI_T}" font-size="12" font-weight="600">▶ Start all</text>')

def facet(y, W, bulk=None, budget=False, search_focus=False):
    s = [f'<rect x="0" y="{y}" width="{W}" height="28" fill="{PAPER}" stroke="{HAIR}" stroke-width="1"/>']
    s.append(f'<rect x="12" y="{y+4}" width="220" height="20" rx="5" fill="{PAPER}" stroke="{RIM if search_focus else HAIR}" stroke-width="{2 if search_focus else 1}"/>')
    s.append(f'<text x="20" y="{y+18}" fill="{MUT}" font-size="11" font-family="{MONO}">Search configurations</text>')
    s.append(f'<text x="240" y="{y+18}" fill="{MUT}" font-size="11" font-family="{MONO}">2 of 3 {"· focused" if search_focus else ""} ✕</text>')
    s.append(f'<rect x="330" y="{y+4}" width="90" height="20" rx="5" fill="{QUIET}" stroke="{HAIR}"/><text x="375" y="{y+18}" text-anchor="middle" fill="{INK}" font-size="11">Group: All</text>')
    s.append(f'<rect x="428" y="{y+4}" width="100" height="20" rx="5" fill="{QUIET}" stroke="{HAIR}"/><text x="478" y="{y+18}" text-anchor="middle" fill="{INK}" font-size="11">Protocol: All</text>')
    s.append(f'<rect x="536" y="{y+4}" width="90" height="20" rx="5" fill="{QUIET}" stroke="{HAIR}"/><text x="581" y="{y+18}" text-anchor="middle" fill="{INK}" font-size="11">Status: All</text>')
    if budget:
        s.append(f'<text x="{W-330}" y="{y+18}" fill="{MUT}" font-size="11" font-family="{MONO}">port-budget: 3 of 499 · 10801–10999 + spill</text>')
    y2 = y + 28
    if bulk:
        s.append(f'<rect x="0" y="{y2}" width="{W}" height="26" fill="{WASH}" stroke="{HAIR}" stroke-width="1"/>')
        s.append(f'<text x="12" y="{y2+17}" fill="{INK}" font-size="11" font-weight="600">{bulk} · Check all visible · Uncheck all · Start checked · Stop checked</text>')
        return y2 + 26
    return y2

def row(y, W, lamp, name, meta, bind, port, btn, btn_kind="start", card=None, chip=None,
        bind_warn=False, port_warn=None, focus_field=None, state_line=None, state_color=MUT, kbd=False):
    fill, edge, ew = QUIET, HAIR, 1
    if card == "selected": fill, edge, ew = WASH, RIM, 2
    if card == "checked": fill, edge, ew = QUIET, RIM, 2
    if card == "both": fill, edge, ew = WASH, RIM, 2
    H = 66
    s = [f'<rect x="12" y="{y}" width="{W-24}" height="{H}" rx="6" fill="{fill}" stroke="{edge}" stroke-width="{ew}"/>']
    s.append(f'<circle cx="34" cy="{y+22}" r="6" fill="{lamp}"/>')
    s.append(f'<rect x="50" y="{y+15}" width="13" height="13" rx="3" fill="{PAPER}" stroke="{MUT}" stroke-width="1"/>')
    if chip: s.append(f'<circle cx="122" cy="{y+22}" r="9" fill="{RIM}"/><text x="122" y="{y+26}" text-anchor="middle" fill="#fff" font-size="10" font-weight="700">{chip}</text>')
    s.append(f'<text x="70" y="{y+27}" fill="{INK}" font-size="14" font-weight="600">{esc(name)}</text>')
    s.append(f'<text x="70" y="{y+47}" fill="{MUT}" font-size="11" font-family="{MONO}">{esc(meta)}</text>')
    fx = 300
    s.append(f'<text x="{fx}" y="{y+20}" fill="{MUT}" font-size="10" font-family="{MONO}">SERVE BIND</text>')
    bw = 118
    be = STOP if bind_warn else HAIR
    s.append(f'<rect x="{fx}" y="{y+24}" width="{bw}" height="24" rx="5" fill="{PAPER}" stroke="{be}" stroke-width="{2 if bind_warn or focus_field=="bind" else 1}"/>')
    if focus_field == "bind": s.append(f'<rect x="{fx-2}" y="{y+22}" width="{bw+4}" height="28" rx="7" fill="none" stroke="{RIM}" stroke-width="2"/>')
    s.append(f'<text x="{fx+8}" y="{y+41}" fill="{INK}" font-size="13" font-family="{MONO}">{esc(bind)}</text>')
    px = fx + bw + 8
    pe = STOP if port_warn else HAIR
    s.append(f'<rect x="{px}" y="{y+24}" width="76" height="24" rx="5" fill="{PAPER}" stroke="{pe}" stroke-width="{2 if port_warn or focus_field=="port" else 1}"/>')
    if focus_field == "port": s.append(f'<rect x="{px-2}" y="{y+22}" width="80" height="28" rx="7" fill="none" stroke="{RIM}" stroke-width="2"/>')
    s.append(f'<text x="{px+8}" y="{y+41}" fill="{INK}" font-size="13" font-family="{MONO}">:{esc(port)}</text>')
    if port_warn: s.append(f'<text x="{px}" y="{y+60}" fill="{STOP}" font-size="10">⚠ {esc(port_warn)}</text>')
    if bind_warn: s.append(f'<text x="{fx}" y="{y+60}" fill="#b45309" font-size="10">leaving loopback — confirm before start</text>')
    bx = px + 84
    if btn_kind == "start":
        s.append(f'<rect x="{bx}" y="{y+24}" width="76" height="24" rx="6" fill="{PRI}"/><text x="{bx+38}" y="{y+41}" text-anchor="middle" fill="{PRI_T}" font-size="12" font-weight="600">Start{"  F5" if kbd else ""}</text>')
    elif btn_kind == "stop":
        s.append(f'<rect x="{bx}" y="{y+24}" width="76" height="24" rx="6" fill="{PAPER}" stroke="{HAIR}"/><text x="{bx+38}" y="{y+41}" text-anchor="middle" fill="{INK}" font-size="12" font-weight="600">■ Stop</text>')
    elif btn_kind == "starting":
        s.append(f'<rect x="{bx}" y="{y+24}" width="96" height="24" rx="6" fill="{PAPER}" stroke="{HAIR}"/><circle cx="{bx+14}" cy="{y+36}" r="5" fill="none" stroke="{MUT}" stroke-width="2"/><text x="{bx+56}" y="{y+41}" text-anchor="middle" fill="{INK}" font-size="12">Starting…</text>')
        bx += 20
    ex = bx + 84
    s.append(f'<rect x="{ex}" y="{y+24}" width="62" height="24" rx="6" fill="{PAPER}" stroke="{HAIR}"/><text x="{ex+31}" y="{y+41}" text-anchor="middle" fill="{INK}" font-size="12">Edit</text>')
    if state_line: s.append(f'<text x="{ex+72}" y="{y+41}" fill="{state_color}" font-size="11" font-family="{MONO}">{esc(state_line)}</text>')
    return "\n".join(s), y + H + 8

def detail(y, W, H, acting, endpoint_ip, endpoint_port, health="health: never tested — press Test to check", usage="usage down 0 B · up 0 B", wrap=False):
    s = [f'<rect x="12" y="{y}" width="{W-24}" height="{H}" rx="6" fill="{PAPER}" stroke="{HAIR}"/>']
    s.append(f'<rect x="24" y="{y+8}" width="210" height="22" rx="5" fill="{QUIET}"/><text x="34" y="{y+23}" fill="{INK}" font-size="11">Details · Health · Presets</text>')
    s.append(f'<text x="24" y="{y+52}" fill="{INK}" font-size="13" font-weight="600">Acting on: {esc(acting)}</text>')
    s.append(f'<text x="24" y="{y+72}" fill="{MUT}" font-size="11" font-family="{MONO}">[vless] [europe] {endpoint_ip}:{endpoint_port}</text>')
    s.append(f'<text x="24" y="{y+94}" fill="{MUT}" font-size="11" font-family="{MONO}">SOCKS</text><text x="90" y="{y+94}" fill="{INK}" font-size="12" font-family="{MONO}">socks5://{endpoint_ip}:{endpoint_port}</text>')
    copyx = 430 if not wrap else 24
    copyy = y + 84 if not wrap else y + 108
    s.append(f'<rect x="{copyx}" y="{copyy}" width="80" height="22" rx="5" fill="{PAPER}" stroke="{HAIR}"/><text x="{copyx+40}" y="{copyy+15}" text-anchor="middle" fill="{INK}" font-size="11">Copy</text>')
    s.append(f'<text x="24" y="{y+116 if not wrap else y+140}" fill="{MUT}" font-size="11" font-family="{MONO}">HTTP&nbsp;&nbsp;&nbsp;</text><text x="90" y="{y+116 if not wrap else y+140}" fill="{INK}" font-size="12" font-family="{MONO}">http://{endpoint_ip}:{endpoint_port}</text>')
    s.append(f'<text x="24" y="{y+136 if not wrap else y+162}" fill="{MUT}" font-size="11" font-family="{MONO}">{esc(health)} · {esc(usage)}</text>')
    oy = y + H - 34
    ops = ["Auto-start ○", "Start F5", "Stop", "Test", "Ping", "Logs", "Copy URLs", "Row actions…"]
    ox = 24
    for op in ops:
        w = 86 if len(op) < 8 else 110
        if ox + w > W - 24:
            if not wrap: break
            ox = 24; oy += 28
        if oy + 24 > y + H - 8: break
        prime = op.startswith("Start F5")
        s.append(f'<rect x="{ox}" y="{oy}" width="{w}" height="24" rx="6" fill="{PRI if prime else PAPER}" stroke="{HAIR}"/><text x="{ox+w/2}" y="{oy+16}" text-anchor="middle" fill="{PRI_T if prime else INK}" font-size="11">{esc(op)}</text>')
        ox += w + 6
    return "\n".join(s)

def status(W, H, left="3 configurations · 2 enabled", center="engine stopped · health: no probes", right="↓ 0 B · ↑ 0 B"):
    return (f'<rect x="0" y="{H-30}" width="{W}" height="30" fill="{CHROME}" stroke="{HAIR}"/>'
        f'<text x="12" y="{H-11}" fill="{MUT}" font-size="11" font-family="{MONO}">{esc(left)}</text>'
        f'<text x="{W//2-140}" y="{H-11}" fill="{MUT}" font-size="11" font-family="{MONO}">{esc(center)}</text>'
        f'<text x="{W-150}" y="{H-11}" fill="{MUT}" font-size="11" font-family="{MONO}">{esc(right)}</text>')

def page(W, H, title, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" font-family="{SANS}">'
        f'<rect width="{W}" height="{H}" fill="{PAPER}"/>{chrome_top(title, W)}{body}{status(W, H)}</svg>')

# ---- v1: idle library, selected + checked, bulk bar, detail ----
W = H = None
W, H = 1000, 700
y = facet(64, W, bulk="1 checked (of 3 visible)")
r, y = row(y+6, W, "#3a3f45", "demo-berlin-p1", "vless · demo-berlin.example.com:443 · europe", "127.0.0.1", "10801", "Start", card="checked", chip="1", state_line="○ Stopped")
b1 = r
r, y = row(y, W, "#3a3f45", "demo-paris-p1", "vless · demo-paris.example.com:443 · europe", "127.0.0.1", "10802", "Start", card="selected", state_line="○ Stopped")
b1 += r
r, y = row(y, W, "#3a3f45", "demo-tokyo-p1", "trojan · demo-tokyo.example.com:443 · asia", "127.0.0.1", "10803", "Start", state_line="○ Stopped")
b1 += r
b1 += detail(y, W, H-30-y-8, "demo-paris-p1", "127.0.0.1", "10802")
open(os.path.join(OUT, "r3-01-idle-selected-checked.svg"), "w").write(page(W, H, "R3-01 · idle · selection wash + checked rim + bulk bar", b1))

# ---- v2: serving states ----
y = facet(64, W, bulk="1 checked (of 3 visible)")
b2, y = row(y+6, W, GO, "demo-berlin-p1", "vless · demo-berlin.example.com:443 · europe", "127.0.0.1", "10801", "Stop", btn_kind="stop", card="checked", chip="1", state_line="● Serving · ↓ 12 MB · ↑ 3 MB", state_color=GO)
r, y = row(y, W, "#3a3f45", "demo-paris-p1", "vless · demo-paris.example.com:443 · europe", "127.0.0.1", "10802", "Starting", btn_kind="starting", card="selected", state_line="… connecting (global busy guard)")
b2 += r
r, y = row(y, W, STOP, "demo-tokyo-p1", "trojan · demo-tokyo.example.com:443 · asia", "127.0.0.1", "10803", "Start", state_line="✕ Endpoint failed — press Test", state_color=STOP)
b2 += r
b2 += detail(y, W, H-30-y-8, "demo-tokyo-p1", "127.0.0.1", "10803", health="✕ Endpoint failed — press Test to check")
open(os.path.join(OUT, "r3-02-serving-starting-failed.svg"), "w").write(page(W, H, "R3-02 · serving green · Starting… spinner · failed red, actions stay enabled", b2))

# ---- v3: validation ----
y = facet(64, W, bulk=None, budget=True)
b3, y = row(y+6, W, "#3a3f45", "demo-berlin-p1", "vless · demo-berlin.example.com:443 · europe", "0.0.0.0", "10801", "Start", bind_warn=True, state_line="○ Stopped · LAN needs naming confirm")
r, y = row(y+8, W, "#3a3f45", "demo-paris-p1", "vless · demo-paris.example.com:443 · europe", "127.0.0.1", "10802", "Start", card="selected", state_line="○ Stopped")
b3 += r
r, y = row(y+8, W, STOP, "demo-tokyo-p1", "trojan · demo-tokyo.example.com:443 · asia", "127.0.0.1", "10802", "Start", port_warn=":10802 in use by demo-paris-p1", state_line="✕ Endpoint failed")
b3 += r
b3 += detail(y+8, W, H-30-y-16, "demo-berlin-p1", "0.0.0.0", "10801", health="⚠ Leaving loopback exposes this machine — start asks for a naming confirm")
open(os.path.join(OUT, "r3-03-validation-collision-lan.svg"), "w").write(page(W, H, "R3-03 · inline collision badge · LAN-bind guard · port budget readout", b3))

# ---- v4: keyboard + focus ----
y = facet(64, W, bulk="1 checked (of 3 visible)", search_focus=True)
b4, y = row(y+6, W, "#3a3f45", "demo-berlin-p1", "vless · demo-berlin.example.com:443 · europe", "127.0.0.1", "10801", "Start", card="checked", chip="1", kbd=True, state_line="○ Stopped")
r, y = row(y, W, "#3a3f45", "demo-paris-p1", "vless · demo-paris.example.com:443 · europe", "127.0.0.1", "10802", "Start", card="selected", kbd=True, focus_field="port", state_line="○ Stopped · port field focused, Enter commits")
b4 += r
r, y = row(y, W, "#3a3f45", "demo-tokyo-p1", "trojan · demo-tokyo.example.com:443 · asia", "127.0.0.1", "10803", "Start", kbd=True, state_line="○ Stopped")
b4 += r
b4 += detail(y, W, H-30-y-8, "demo-paris-p1", "127.0.0.1", "10802")
open(os.path.join(OUT, "r3-04-focus-keyboard.svg"), "w").write(page(W, H, "R3-04 · focus ring + Kbd chips + search focus + check numeral", b4))

# ---- v5: 880x620 minimum frame ----
W5, H5 = 880, 620
y = facet(64, W5, bulk="1 checked (of 3 visible)")
b5, y = row(y+6, W5, "#3a3f45", "demo-berlin-p1", "vless · demo-berlin…:443 · europe", "127.0.0.1", "10801", "Start", card="checked", chip="1", state_line="○ Stopped")
r, y = row(y, W5, "#3a3f45", "demo-paris-p1", "vless · demo-paris.example…:443 · europe", "127.0.0.1", "10802", "Start", card="selected", state_line="○ Stopped")
b5 += r
b5 += detail(y, W5, H5-30-y-8, "demo-paris-p1", "127.0.0.1", "10802", wrap=True)
open(os.path.join(OUT, "r3-05-minimum-880x620.svg"), "w").write(page(W5, H5, "R3-05 · 880×620 minimum · operate row wraps, nothing clips", b5))

print("round3 done:", sorted(os.listdir(OUT)))
