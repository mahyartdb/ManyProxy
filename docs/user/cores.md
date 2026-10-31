# Cores ManyProxy runs

ManyProxy does not include VPN cores. It runs executables you place in the app
folder yourself, beside `ManyProxy.exe`:

- **sing-box** (`sing-box.exe`) — the stock core for plain vless, vmess, trojan,
  hysteria, tuic, socks, http, anytls, naive, shadowsocks and more.
- **sing-box-awg** (`sing-box-awg.exe`) — an AmneziaWG-capable sing-box fork,
  needed only by AmneziaWG, Noise and XHTTP rows. Stock configs keep working
  without it.
- **xray** (`xray.exe`) — Xray-core, needed only by rows that require the xray
  sidecar (Xray-only rows).

# Where to get them manually

Download the executables from their official releases yourself:

- sing-box: <https://github.com/SagerNet/sing-box>
- Xray-core: <https://github.com/XTLS/Xray-core>
- An AmneziaWG-capable sing-box fork is published by the Amnezia project on
  GitHub (its releases page hosts Windows builds of `sing-box-awg`).

Unzip the download and put each core executable next to `ManyProxy.exe`. The
Settings dialog's three engine rows should then all read `ready`.

ManyProxy never downloads executables silently and never searches PATH — you
choose the file, you place it, and nothing arrives on its own.
