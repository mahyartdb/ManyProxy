# Security

ManyProxy is a local-only Windows proxy library. It stores configurations and
serves each one on its own loopback endpoint (`127.0.0.1`); it has no server
component, no telemetry, and never changes the system proxy, DNS, routes, or
installs a service or driver. Imported configurations, subscription bodies, and
local files are still untrusted input, and the app handles proxy credentials.

## Supported versions

Only the current GitHub release is supported. The beta ships portable ZIPs and
has no long-term support branches; reproduce a report on the latest release
before sending it.

## Reporting a vulnerability

Do not open a public issue for a security bug.

Use GitHub's private vulnerability reporting:
<https://github.com/mahyartdb/ManyProxy/security/advisories/new>
(the repository's **Security** tab → **Report a vulnerability**). The report
stays private between you and the maintainer.

Include, as far as you can:

- the ManyProxy version (About dialog) and your Windows version;
- what you did, what happened, and what you expected instead;
- the smallest input or configuration that reproduces it — synthetic
  credentials are fine; do not send real server addresses, UUIDs, passwords,
  or private keys;
- the impact you believe it has, plus any logs or screenshots that help.

## What to expect

This is a best-effort project with no hard SLA. The maintainer aims to
acknowledge a report within a week and to follow up with a fix or a decision
after triage. Please leave a reasonable window before any public write-up.

## Beta limits, stated honestly

- **Unsigned binaries.** Releases are not code-signed, so SmartScreen and
  antivirus heuristics routinely flag a fresh executable that spawns sidecars
  and listens on loopback.
- **SHA-256 hashes only.** Every release publishes `SHA256SUMS.txt` for the ZIP
  and its bundled binaries. Verify it; it is the integrity floor, not a
  signature.
- **No auto-update.** The app reads the release feed at most once a week and
  opens the release page in your browser. It never downloads or replaces an
  executable; updating means replacing the folder by hand.
