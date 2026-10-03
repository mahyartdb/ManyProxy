# Research: How agents can review and test Windows app UI like Playwright does for the web

**Date:** 2026-10-03
**Decision this informs:** which stack to build an agent-driven Windows desktop UI review/testing harness on.
**Confidence overall:** medium-high (mechanisms and maintenance status are primary-sourced; no harness was executed this run)
**Raw evidence:** `research/findings/01-native-automation.md` (20 sources), `02-vision-agents.md` (20 sources), `03-test-harnesses.md` (20 sources) — all claims below trace to one of those files.

## TL;DR

There is no Playwright for native Windows apps, and there will not be one soon: Playwright is browser-scoped by design, and the nearest desktop bridge (`FlaUI.WebDriver`) is self-labelled experimental. What exists instead is a three-layer stack that reproduces ~90% of the Playwright contract — (1) the Windows UI Automation tree as the DOM, with driver protocols (Appium `windows` / WinAppDriver / NovaWindows) as the "browser"; (2) Playwright itself attached over CDP for any web content inside the window (Electron, WebView2); (3) vision grounding (OmniParser / cua) as fallback for custom-drawn UI the tree cannot see. The decisive empirical fact: on Windows Agent Arena, vision-only scored 12.5% vs the same agent + UIA structure at 19.5%, against a 74.5% human baseline — structure beats pixels, and neither is close to human yet, so assertions must be scripted state checks, not VLM judgment.

## Key findings

### 1. The UIA tree is the DOM — with weaker selectors than CSS

Microsoft explicitly designed UI Automation "to provide robust functionality for automated testing scenarios." The tree is lazily built and mutates (the DOM analog), and exposes three filtered views — raw, control ("most interesting to automated testing applications"), content — plus control patterns as the behavior interface (Toggle/Invoke/Value ≈ ARIA roles + actions). **Fact** — [Tree Overview](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-treeoverview), [Using UIA for Automated Testing](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-usefortesting).

Locator mapping: `AutomationId` ≈ `data-testid`, `Name` ≈ accessible name, `ControlType` ≈ ARIA role, `ClassName` ≈ CSS class, `RuntimeId` ≈ an ephemeral node ref that creates Playwright-style staleness. But MS states `AutomationId` support "is not required", is sibling-scoped, and is "not guaranteed to be stable across different releases or builds"; `Name` is localized. Raw UIA has no XPath at all — XPath 1.0 exists only at the driver layer. **Fact** for the property text; **Inference** for the analogy. **Net: desktop selectors are strictly weaker than web selectors**, so test IDs must be treated as a first-party contract, same discipline as `data-testid` but with no compiler enforcing it. — [Key Properties](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-usefortesting)

### 2. The driver layer is alive only as a proxy over a dead server

- **WinAppDriver** (Microsoft): speaks real Selenium/WebDriver REST on `127.0.0.1:4723`, but last stable release **2020-11-05**, closed-source, 1,155 open issues, GitHub `activeRepoStatus:false`. **Fact** — [releases](https://github.com/microsoft/WinAppDriver/releases), [FAQ](https://github.com/microsoft/WinAppDriver/blob/master/Docs/FAQ.md).
- **`appium-windows-driver`** v6.3.0 published **2026-09-14**, ~423k monthly downloads — actively maintained, but it is explicitly "a proxy to Microsoft's WinAppDriver server," and its own README says: "WinAppDriver server has not been maintained by Microsoft for years. Consider trying NovaWindows Driver." **Fact** — [npm](https://registry.npmjs.org/appium-windows-driver/latest), [README](https://github.com/appium/appium-windows-driver).
- **NovaWindows** (`appium driver install --source=npm appium-novawindows-driver`): open-source UIA3 driver, last push **2026-08-27**, adds RawView element access, direct control-pattern commands (`windows: invoke/toggle/setValue/...`), raw UIA `PropertyCondition` locators. Vendor reliability claims are unverified (80 stars, early-stage). **Fact** for existence/dates; **unverified** for efficacy. — [README](https://github.com/AutomateThePlanet/appium-novawindows-driver)
- **Inference:** the de-facto 2026 recipe is Appium 3 + `appium-windows-driver` now, NovaWindows as the watch-list replacement, WinAppDriver only as the underlying runtime both depend on.

### 3. Non-C# languages reach the tree through wrappers, not through Playwright

| Language | Library | Status (verified this run) |
|---|---|---|
| C# | **FlaUI** (`FlaUI.UIA3`) | v5.0.0 2025-02-25, last commit **2026-08-13** "Fix tests in CI" — runs its own UI tests in CI |
| Python | **pywinauto** 0.6.9 / **uiautomation** 2.0.29 | 2025-01-06 / 2025-08-05, both pushing in 2026 |
| Rust | **`uiautomation`** crate | 0.25.1, updated **2026-09-04**; `windows` crate exposes `Win32_UI_Accessibility` |
| Node | none dominant | `node-uiautomation` 404s; niche is fragmenting into MCP wrappers (`uia-mcp`, etc., hundreds of monthly downloads) |
| Any | **FlaUI.WebDriver** | W3C WebDriver bridge — **"EXPERIMENTAL. It is not feature complete"** |

**Fact** — NuGet/PyPI/crates.io/npm registries + GitHub API, fetched 2026-10-03; sources 15, 17, 19, 20 in `01-native-automation.md`. **Inference:** for an agent harness the choice is forced by the agent's own language — a Rust/TS agent should go through Appium's WebDriver protocol or an MCP server, not through .NET.

### 4. Where Playwright *does* apply: web content inside the window

- **Electron:** `playwright._electron` exists but is documented as **experimental**; it launches via CDP. Native OS dialogs (`dialog.showOpenDialog`) are explicitly outside Playwright's reach. — [Playwright Electron docs](https://playwright.dev/docs/api/class-electron)
- **WebView2:** attach with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` (or the HKCU policy key), then `chromium.connectOverCDP(port)`. Only one WebView2 control can expose a CDP port. — [Microsoft Learn](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code)
- **Chromium-based content generally:** Chrome does not expose its accessibility tree unless launched with `--force-renderer-accessibility`. — [FlaUI wiki FAQ](https://github.com/FlaUI/FlaUI/wiki/FAQ)
- **Inference:** a hybrid app gets *two* harnesses — Playwright for the document, UIA/driver for the native chrome, title bar, menus, and file pickers. This split is the normal case, not an edge case.

### 5. Vision-only agents measurably underperform structured ones on Windows

Windows Agent Arena (Microsoft, 150+ tasks, Sep 2024), Table 4 — **Fact**, [arXiv 2409.08264](https://arxiv.org/abs/2409.08264):

| Configuration | Task success |
|---|---|
| Vision-only: OmniParser SoM + GPT-4V | **12.5%** |
| Hybrid: UIA markers + OmniParser + GPT-4V | **19.5%** |
| Human (unassisted) | **74.5%** |

Adding UIA markers lifted performance "by 57% for Omniparser, 52% for open-sourced models, and 15% for proprietary pixel models" — and the same paper documents the cost: querying UIA "can take from a few seconds up to several minutes" on complex screens. The WAA README rates the `win32` a11y backend "faster, less accurate" than `uia` "slower, more accurate."

Benchmark trajectory (all **Fact**, dates matter — these numbers are not cross-comparable): OSWorld best **12.24%** (Apr 2024, human 72.36%) → OSWorld-Verified best **60.76%** (Jul 2025) after 300+ task fixes; AndroidWorld best **30.6%** at launch (2024).

**Inference:** current SOTA on Windows is hybrid — structured tree for grounding and verification, vision as fallback. Every top OSWorld-Verified entry is a framework (CoACT-1, Agent S2.5), not a bare vision model.

### 6. Verification must be scripted state, not VLM judgment

OSWorld's scoring is "a custom **execution-based evaluation script** for reliable, reproducible evaluation" — each task asserts post-conditions programmatically. Minitap's AndroidWorld failure analysis quantifies the value: **deterministic post-validation of text input added +7 points**, against failure modes it names as "silent text input failures undetected by the agent." **Fact** — [OSWorld](https://github.com/xlang-ai/OSWorld), [arXiv 2602.07787](https://arxiv.org/abs/2602.07787).

**Design rule (Inference):** use the tree to *act*, scripted UIA/state queries to *assert*, and vision only to *review* (qualitative critique, layout judgement) — never to grade pass/fail.

### 7. Screenshot capture and the DPI trap

- Per-window capture: **Windows.Graphics.Capture** (modern, GPU-accurate) or **PrintWindow** (synchronous; the target app must process `WM_PRINT`, so GPU-composited apps often yield blanks — widely reported, *not directly sourced*). DXGI duplication is desktop-wide only. **Fact** for API semantics — [PrintWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-printwindow), [WGC](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture).
- **DPI is the #1 source of baseline mismatch.** Microsoft: "The screen now contains two coordinate systems, physical and logical… at 96 dpi the button is at (100, 48); at 120 dpi it is at (125, 60)." A baseline captured at 100% will not match a 125/150% CI machine, and coordinate clicks drift by the scale factor. — [Screen Scaling](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-screenscaling)
- **Rule:** pin DPI (100%) and pixel dimensions on every machine that captures baselines, or diff with a scale-tolerant algorithm. FlaUI ships `Capture`/`CaptureImage` + `VideoRecorder` (ffmpeg) as first-party capture for failure artifacts.

### 8. CI: the failure mode is the session, not the framework

- Windows services run in **Session 0**, which has had no interactive UI access since Windows 10 1803 (UI0Detect removed, `NoInteractiveServices` defaults to 1). The app under test **must** run in an interactive session. — [Interactive services](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services)
- Hosted runners cannot run interactive UI tests by default: `actions/runner-images` issue #3180 documents images that "cannot run interactive tests (for UI Tests)" without "Configure VM to run interactively"; the community workaround is autologon + manual `WinAppDriver.exe` start (discussion #36704); WinAppDriver issue #1821 is the canonical "works locally, fails on the agent."
- **Counter-evidence it works:** FlaUI's own master was still fixing CI-run UI tests in Aug 2026, on AppVeyor.
- **Recipe (Inference):** self-hosted or autologoned Windows runner, pinned DPI, session kept logged-on, screenshots as CI artifacts, tests tolerant of a shared desktop. There is no Windows equivalent of `headless: true`; nearest are `mstsc /admin`, offscreen `PrintWindow`, or a VM.
- Observation tools: `mstsc /shadow:<id> /control` to watch/steer a session without displacing the logged-on user.

### 9. Agent-facing tooling that exists today (installable now)

| Tool | What it gives an agent | Evidence |
|---|---|---|
| **cua-driver / cua** (trycua/cua) | MCP + CLI + SDK; macOS/**Windows**/Linux native app control; background delivery without stealing focus; `install.ps1`; optional OmniParser+PP-OCR perception bundle. **Already configured in this environment (57 tools).** | [repo](https://github.com/trycua/cua) — 27.8k★, pushed this run's date; per-action Windows parity **unverified** |
| **playwright-mcp** (microsoft) | Browser-only, a11y-tree-first, explicitly "avoids ambiguity common with screenshot-based approaches"; vision opt-in via `--caps vision` | [repo](https://github.com/microsoft/playwright-mcp) — 37.7k★ |
| **desktop-touch-mcp** | Windows 10/11 Rust UIA engine; "semantic discover-then-act… not pixel coordinates"; documents its own failure classes (negative multi-monitor coords, stale clicks, wrong-window focus) | 21★, npm v2.0.0 2026-09-24 — self-reported |
| **Microsoft UFO²/UFO³** | The research-grade hybrid Windows agent: UIA + Win32 + WinCOM + visual detection, "Hybrid control detection for robustness", speculative multi-action batching (−51% LLM calls) | [repo](https://github.com/microsoft/UFO) — 9.9k★, pushed 2026-09-29 |
| **Power Automate Desktop** | Free tier: "authoring and running desktop flows locally (attended)" — RPA, not assertion-first | [MS licensing](https://learn.microsoft.com/en-us/power-platform/admin/power-automate-licensing/types) |

Anthropic's computer-use reference is **Linux-in-Docker**, not Windows. Microsoft's `windows-agent-adapter` could not be found (negative search — treated as unverified belief). No AI-testing vendor (QA Wolf, Testim, Momentic) claims native Windows desktop support.

### 10. Documented failure modes of the vision path

Imprecise/hallucinated Set-of-Mark boxes (WAA paper); screenshot downscale to 1024×768 + coordinate remapping required for accuracy (Anthropic reference); negative multi-monitor coordinates, stale clicks, wrong-window typing; silent text-input failures; "high behavioral standard deviation in computer use agents" — "even 90% correct knowledge yields only 41% execution success rate" (UI-Evol); ~**$100** VLM API cost per full 150-task WAA run (GPT-4o) vs ~$15 (GPT-4o-mini). All **Fact** with sources in `02-vision-agents.md` §5.

## Solutions (ranked)

**A. Hybrid driver stack — recommended for testing.**
Appium 3 + `appium-windows-driver` (or FlaUI if you stay in .NET) for locate/act; **scripted UIA queries for assertions**; Playwright-over-CDP for Electron/WebView2 content; Windows.Graphics.Capture screenshots with DPI pinned; CI on an autologoned Windows runner. This is the only option that is both deterministic and available today. Effort: ~2–3 days for a working slice, ~1–2 weeks for a maintainable harness.

**B. MCP computer-use layer — recommended for UI *review*.**
Drive the app with `cua-driver` (already installed here) + screenshots, and have the agent critique layout/copy/state transitions qualitatively. Pair with a UIA tree dump so the agent reasons over structure, not just pixels. Use this for review, **not** for pass/fail gating. Effort: hours.

**C. Build a thin "Playwright-shaped" wrapper over UIA** — auto-waiting locators + `expect(locator).toBeVisible()`-style assertions + trace (per-step screenshots + tree snapshots). The gap is real and nothing maintained fills it; FlaUI.WebDriver proves the shape but is experimental. Only worth it if A proves too verbose. Effort: 1–2 weeks.

**D. Power Automate Desktop / RPA recorders** — free and fast for scripted flows; not assertion-first, poor agent integration. Use only for throwaway checks.

**Do not:** gate CI on vision-only agent success (12.5–19.5% reliable), or assume Playwright can reach native chrome (it cannot).

## Contradictions / uncertainty

- **WinAppDriver "dead" vs alive:** server binary stale since 2020 (Fact), repo not archived with a 2025-04 housekeeping commit (Fact), and its runtime is still what the actively-released Appium driver proxies (Fact). Resolution: engine stale, protocol layer alive.
- **NovaWindows** performance claims are vendor assertions; no independent benchmark found.
- **TagUI "discontinued"** per listicles vs `pushed_at 2026-09-20` (Fact) — "alive but possibly quiet" is the defensible reading.
- **cua Windows depth:** top-level README claims Windows + Windows CI; the `cua-driver` README's first line says "drives native macOS apps". Per-action Windows parity unverified.
- **Benchmark numbers are not cross-comparable** across papers (different step budgets, tasks, dates). Always cite date + verified status.
- One research agent reported its web-search tool returning zero results that run; its findings rest on direct primary-source fetches only. Search-engine corroboration was unavailable for that stream.

## Evidence against

1. **`cited`, confidence high** — Structured automation on Windows is structurally fragile: selectors are optional/unstable (MS's own words), custom controls are "largely opaque… only basic information available from the window handle," and elevation requires a signed+installed harness with `uiAccess=true`. A Playwright-grade experience may be unbuildable for apps that don't cooperate.
2. **`cited`, confidence high** — Even the best-researched agents hit 19.5–60.8% where humans hit 72–74.5%. If the goal is automated pass/fail, agent-driven testing is not yet reliable enough; if the goal is assisted review, it is. The goal must be chosen before building.
3. **`cited`, confidence medium** — CI is a genuine tax: interactive-session workarounds are undocumented community folklore (autologon, `WinAppDriver.exe` manual start), and hosted runners fail by default. Budget for a self-hosted logged-on Windows box.
4. **`cited`, confidence medium** — Cost/latency: UIA queries "can take seconds to minutes" on complex screens; full vision-agent benchmark runs cost ~$100 in API charges. Neither is free.
5. **`ASSUMPTION`, confidence medium** — Maintenance risk concentrates in one closed-source binary (WinAppDriver) that both driver options depend on. If Microsoft archives it, both `appium-windows-driver` and every WinAppDriver deployment lose their runtime.

## Gaps

- No harness was executed — all findings are documentation/registry-derived. A 1-hour hands-on spike would settle: cua-driver's actual Windows action parity, `appium-windows-driver` latency on a real app, and PrintWindow blank-frame behavior on the target app.
- Per-framework accessibility coverage (WinUI 3, Qt, Electron specifics) only partially sourced; DirectX/canvas opacity rests on one providers-overview quote.
- Commercial tools (Ranorex, TestComplete) and visual-diff services (Applitools, Percy) for native desktop — unchecked.
- WebDriver BiDi relevance to UIA sessions — unchecked.
- No desktop analogue of Playwright's `trace.zip` viewer was found; closest are FlaUI `VideoRecorder` + per-step screenshots.
- Windows-specific 2026 agent SOTA numbers — WAA has no newer leaderboard.

## Recommended next steps

1. Run solution B first (hours): use the already-installed `cua-driver` to open, screenshot, and dump the target app's UIA tree — decide from that whether structure or vision carries the review.
2. Spike solution A for 1 hour: `npm i -D @appium/cli && appium driver install windows`, launch the app, run one `findElement` + one assertion.
3. Decide the pass/fail question: scripted assertions (A) vs agent judgement (B) — they are different products.
4. If both pay off, design solution C's API surface (locator + auto-wait + assertion) before writing it.

## Sources

Full numbered source lists with dates and confidence ratings live in the findings files:

- `research/findings/01-native-automation.md` — 20 sources (MS Learn, GitHub API via `gh`, npm/PyPI/crates.io/NuGet/docs.rs).
- `research/findings/02-vision-agents.md` — 20 sources (arXiv, GitHub API, npm/PyPI).
- `research/findings/03-test-harnesses.md` — 20 sources (Playwright/Electron/MS Learn first-party docs, GitHub API, vendor pages).

Top load-bearing primary sources: [MS UI Automation for Testing](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-usefortesting) · [UIA Providers Overview](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-providersoverview) · [UIA Screen Scaling](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-screenscaling) · [WinAppDriver](https://github.com/microsoft/WinAppDriver) · [appium-windows-driver](https://github.com/appium/appium-windows-driver) · [FlaUI](https://github.com/FlaUI/FlaUI) · [Windows Agent Arena](https://arxiv.org/abs/2409.08264) · [OSWorld](https://github.com/xlang-ai/OSWorld) · [microsoft/UFO](https://github.com/microsoft/UFO) · [playwright-mcp](https://github.com/microsoft/playwright-mcp) · [trycua/cua](https://github.com/trycua/cua) · [Playwright Electron](https://playwright.dev/docs/api/class-electron) · [WebView2 CDP debugging](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code) · [Session 0 / interactive services](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services)
