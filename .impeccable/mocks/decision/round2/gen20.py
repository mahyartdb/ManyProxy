#!/usr/bin/env python3
"""Generate 20 direction mocks: 5 worlds x 4 inline-edit patterns. Single-line rows."""
import os

OUT = r"E:\Code\ManyProxy\.impeccable\mocks\decision\round2"
os.makedirs(OUT, exist_ok=True)

ROWS = [
    dict(name="demo-berlin-p1", proto="vless", grp="europe", up="demo-berlin.example.com", uport="443", port="10801", state="stopped", state_txt="○ Stopped"),
    dict(name="demo-paris-p1", proto="vless", grp="europe", up="demo-paris.example.com", uport="443", port="10802", state="selected", state_txt="○ Stopped"),
    dict(name="demo-tokyo-p1", proto="trojan", grp="asia", up="demo-tokyo.example.com", uport="443", port="10803", state="failed", state_txt="✕ Failed"),
]

WORLDS = {
    "A-dark":  dict(bg="#0d0f12", chrome="#14171c", card="#f4f1e6", card_ink="#141414", sub="#55534a",
                    field="#ffffff", field_ink="#141414", field_edge="#b9b4a4", text="#f2f4f6", dim="#9aa3ad",
                    mono="Consolas, Cascadia Mono, monospace", sans="Segoe UI, system-ui, sans-serif",
                    title="A · Strip-Chart Bench", start="#f2f4f6", start_ink="#141414"),
    "A-light": dict(bg="#f4f4f5", chrome="#e8e8ea", card="#ffffff", card_ink="#141414", sub="#6b7280",
                    field="#f4f1e6", field_ink="#141414", field_edge="#c9c4b2", text="#141414", dim="#6b7280",
                    mono="Consolas, Cascadia Mono, monospace", sans="Segoe UI, system-ui, sans-serif",
                    title="A · Strip-Chart Bench (light)", start="#141414", start_ink="#ffffff"),
    "B-patch": dict(bg="#101216", chrome="#1a1e24", card="#171b21", card_ink="#e8ebee", sub="#7d8590",
                    field="#0b0d10", field_ink="#e8ebee", field_edge="#4b5563", text="#eef1f4", dim="#9aa3ad",
                    mono="Consolas, Cascadia Mono, monospace", sans="Segoe UI, system-ui, sans-serif",
                    title="B · Patch-Bay Wall", start="#22c55e", start_ink="#06130b"),
    "C-table": dict(bg="#d9a62e", chrome="#2b2f36", card="#f5edd8", card_ink="#1c1c1c", sub="#4a463c",
                    field="#f5edd8", field_ink="#1c1c1c", field_edge="#1c1c1c", text="#1c1c1c", dim="#4a463c",
                    mono="'Arial Narrow', sans-serif", sans="'Arial Narrow', sans-serif",
                    title="C · Timetable Rack", start="#101418", start_ink="#f5edd8"),
    "D-gate":  dict(bg="#e9ebee", chrome="#111418", card="#ffffff", card_ink="#111111", sub="#666666",
                    field="#f2f3f4", field_ink="#111111", field_edge="#b9bec6", text="#111418", dim="#6b7280",
                    mono="Consolas, monospace", sans="Segoe UI, system-ui, sans-serif",
                    title="D · Gate Board", start="#111418", start_ink="#ffffff"),
}

PATTERNS = {
    "P1-fields": "always-on addr+port fields",
    "P2-expand": "selected row expands editors",
    "P3-dual":   "upstream group | loopback group",
    "P4-compact":"compact text + edit affordance",
}

STATE_DOT = {"stopped": None, "selected": None, "failed": "#ef4444"}
STATE_GOOD = "#22c55e"

def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def field(x, y, w, h, label, wld, ink=None):
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="5" fill="{wld["field"]}" stroke="{wld["field_edge"]}" stroke-width="1"/>'
            f'<text x="{x+8}" y="{y+h-8}" fill="{ink or wld["field_ink"]}" font-size="13" font-family="{wld["mono"]}">{esc(label)}</text>')

def row_svg(w, p, r, y, sel):
    wld = WORLDS[w]
    H = 64
    s = []
    dot = "#3a3f45" if r["state"] != "failed" else "#ef4444"
    if r["state"] == "selected":
        s.append(f'<rect x="16" y="{y}" width="968" height="{H}" rx="7" fill="{wld["card"]}" stroke="#1d4ed8" stroke-width="3"/>')
    else:
        s.append(f'<rect x="16" y="{y}" width="968" height="{H}" rx="7" fill="{wld["card"]}" />')
    s.append(f'<circle cx="36" cy="{y+22}" r="6" fill="{dot}"/>')
    s.append(f'<text x="50" y="{y+27}" fill="{wld["card_ink"]}" font-size="14" font-weight="700" font-family="{wld["sans"]}">{esc(r["name"])}</text>')
    s.append(f'<text x="50" y="{y+48}" fill="{wld["sub"]}" font-size="11" font-family="{wld["mono"]}">{r["proto"]} · {r["grp"]} · {esc(r["state_txt"])}</text>')
    if p == "P1-fields":
        s.append(field(268, y+12, 300, 26, r["up"], wld))
        s.append(field(574, y+12, 70, 26, r["uport"], wld))
        s.append(field(650, y+12, 84, 26, ":" + r["port"], wld))
        s.append(f'<rect x="742" y="{y+12}" width="86" height="26" rx="6" fill="{wld["start"]}"/><text x="785" y="{y+30}" text-anchor="middle" fill="{wld["start_ink"]}" font-size="12" font-weight="700">Start</text>')
        s.append(f'<rect x="836" y="{y+12}" width="58" height="26" rx="6" fill="none" stroke="{wld["sub"]}" stroke-width="1"/><text x="865" y="{y+30}" text-anchor="middle" fill="{wld["sub"]}" font-size="12">Test</text>')
        s.append(f'<rect x="902" y="{y+12}" width="66" height="26" rx="6" fill="none" stroke="{wld["sub"]}" stroke-width="1"/><text x="935" y="{y+30}" text-anchor="middle" fill="{wld["sub"]}" font-size="12">Copy</text>')
    elif p == "P2-expand":
        if sel:
            s.append(field(268, y+12, 300, 26, r["up"], wld))
            s.append(field(574, y+12, 70, 26, r["uport"], wld))
            s.append(field(650, y+12, 84, 26, ":" + r["port"], wld))
            s.append(f'<rect x="742" y="{y+12}" width="86" height="26" rx="6" fill="{wld["start"]}"/><text x="785" y="{y+30}" text-anchor="middle" fill="{wld["start_ink"]}" font-size="12" font-weight="700">Start</text>')
            s.append(f'<rect x="836" y="{y+12}" width="128" height="26" rx="6" fill="none" stroke="{wld["sub"]}" stroke-width="1"/><text x="900" y="{y+30}" text-anchor="middle" fill="{wld["sub"]}" font-size="12">Test · Copy</text>')
        else:
            s.append(f'<text x="268" y="{y+30}" fill="{wld["sub"]}" font-size="13" font-family="{wld["mono"]}">{esc(r["up"])}:{r["uport"]} → :{r["port"]}</text>')
            s.append(f'<rect x="742" y="{y+12}" width="86" height="26" rx="6" fill="{wld["start"]}"/><text x="785" y="{y+30}" text-anchor="middle" fill="{wld["start_ink"]}" font-size="12" font-weight="700">Start</text>')
            s.append(f'<text x="900" y="{y+30}" fill="{wld["sub"]}" font-size="12" text-anchor="middle">expand ▸</text>')
    elif p == "P3-dual":
        s.append(f'<text x="264" y="{y+22}" fill="{wld["sub"]}" font-size="10" font-family="{wld["mono"]}">UPSTREAM</text>')
        s.append(field(264, y+26, 240, 24, r["up"], wld))
        s.append(field(510, y+26, 64, 24, r["uport"], wld))
        s.append(f'<text x="590" y="{y+22}" fill="{wld["sub"]}" font-size="10" font-family="{wld["mono"]}">LOOPBACK</text>')
        s.append(field(590, y+26, 80, 24, "127.0.0.1", wld))
        s.append(field(676, y+26, 74, 24, ":" + r["port"], wld))
        s.append(f'<rect x="758" y="{y+26}" width="86" height="24" rx="6" fill="{wld["start"]}"/><text x="801" y="{y+43}" text-anchor="middle" fill="{wld["start_ink"]}" font-size="12" font-weight="700">Start</text>')
        s.append(f'<rect x="852" y="{y+26}" width="58" height="24" rx="6" fill="none" stroke="{wld["sub"]}" stroke-width="1"/><text x="881" y="{y+43}" text-anchor="middle" fill="{wld["sub"]}" font-size="12">Test</text>')
        s.append(f'<rect x="918" y="{y+26}" width="50" height="24" rx="6" fill="none" stroke="{wld["sub"]}" stroke-width="1"/><text x="943" y="{y+43}" text-anchor="middle" fill="{wld["sub"]}" font-size="12">⎘</text>')
    else:  # P4-compact
        s.append(f'<text x="268" y="{y+30}" fill="{wld["card_ink"]}" font-size="13" font-family="{wld["mono"]}">{esc(r["up"])}:{r["uport"]} → :{r["port"]}  ✎</text>')
        s.append(f'<rect x="742" y="{y+12}" width="70" height="26" rx="13" fill="{wld["start"]}"/><text x="777" y="{y+30}" text-anchor="middle" fill="{wld["start_ink"]}" font-size="12" font-weight="700">▶</text>')
        s.append(f'<text x="830" y="{y+30}" fill="{wld["sub"]}" font-size="12">Test · ⎘</text>')
        if r["state"] == "failed":
            s.append(f'<text x="902" y="{y+30}" fill="#ef4444" font-size="12">⚠ port</text>')
    return "\n".join(s)

def page(w, p, idx):
    wld = WORLDS[w]
    ys = [100, 176, 252]
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="700" font-family="{wld["sans"]}">',
             f'<rect width="1000" height="700" fill="{wld["bg"]}"/>',
             f'<rect x="0" y="0" width="1000" height="44" fill="{wld["chrome"]}"/>',
             f'<text x="16" y="27" fill="{"#f5edd8" if w in ("C-table",) else ("#141414" if w=="A-light" else ("#111418" if w=="D-gate" else "#eef1f4"))}" font-size="14" font-weight="600">ManyProxy — {wld["title"]} · {p} {PATTERNS[p]} · #{idx:02d}</text>',
             f'<text x="16" y="70" fill="{wld["dim"]}" font-size="12" font-family="{wld["mono"]}">Search · Group: All · Protocol: All · Status: All · 3 of 3 · inline addr+port+start in one row</text>']
    for r, y in zip(ROWS, ys):
        parts.append(row_svg(w, p, r, y, r["state"] == "selected"))
    parts.append(f'<rect x="16" y="330" width="968" height="120" rx="7" fill="{wld["chrome"] if w!="C-table" else "#101418"}"/>')
    parts.append(f'<text x="32" y="356" fill="{wld["dim"] if w!="C-table" else "#f5edd8"}" font-size="12" font-family="{wld["mono"]}">Acting on: demo-paris-p1 · edits validate inline (port budget · collisions · loopback guard) · Enter commits · Esc reverts</text>')
    parts.append(f'<text x="32" y="380" fill="{wld["dim"] if w!="C-table" else "#f5edd8"}" font-size="12" font-family="{wld["mono"]}">SOCKS socks5://127.0.0.1:10802 [Copy] · HTTP http://127.0.0.1:10802 [Copy] · health: never tested — press Test</text>')
    parts.append(f'<text x="32" y="404" fill="{wld["dim"] if w!="C-table" else "#f5edd8"}" font-size="12" font-family="{wld["mono"]}">Start F5 · Stop · Test Ctrl+T · Ping · Logs · Copy URLs Ctrl+U · Row actions…</text>')
    parts.append(f'<text x="32" y="428" fill="{wld["dim"] if w!="C-table" else "#f5edd8"}" font-size="12" font-family="{wld["mono"]}">secrets never shown inline · upstream edits merge into stored params like the guided Add form</text>')
    parts.append(f'<rect x="0" y="662" width="1000" height="38" fill="{wld["chrome"]}"/>')
    parts.append(f'<text x="16" y="685" fill="{wld["dim"] if w!="C-table" else "#f5edd8"}" font-size="12" font-family="{wld["mono"]}">3 configurations · 2 enabled | engine stopped | health: no probes | ↓ 0 B · ↑ 0 B</text>')
    parts.append('</svg>')
    return "\n".join(parts)

i = 0
files = []
for w in ["A-dark", "A-light", "B-patch", "C-table", "D-gate"]:
    for p in ["P1-fields", "P2-expand", "P3-dual", "P4-compact"]:
        i += 1
        fn = f"{i:02d}-{w}-{p}.svg"
        with open(os.path.join(OUT, fn), "w", encoding="utf-8") as f:
            f.write(page(w, p, i))
        files.append(fn)
print(f"wrote {len(files)} files to {OUT}")
for fn in files:
    print(fn)
