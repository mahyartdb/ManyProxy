# 03 — Test harnesses for a Windows desktop app (Playwright-parity question)

Research date: 2026-10-03. Every claim below carries an inline source link retrieved during this run.
Epistemic labels: **Fact** = direct source; **Inference** = my reasoning over facts; **Assumption** = unverified.

## Sub-question findings

### SQ1 — Test frameworks wrapping UIA for desktop apps (maintenance state)

**FlaUI (.NET)** — *alive in 2026.*
- Latest release **v5.0.0 published 2025-02-25** (breaking: drops .NET Standard / .NET Core 3.1 / .NET 5, adds .NET 8, adds "screenshot to testresult for XML report"). Source: [GitHub API — FlaUI releases](https://api.github.com/repos/FlaUI/FlaUI/releases?per_page=3) (Fact).
- Last commit on master: **2026-08-13**, message "Fix tests in CI (#746) ... wait for notepad window ... Improve the ffmpeg download" — i.e. it runs its own UI tests in CI as of Aug 2026. Source: [GitHub API — FlaUI commits](https://api.github.com/repos/FlaUI/FlaUI/commits?per_page=1) (Fact).
- FlaUI is "a .NET library which helps with automated UI testing of Windows applications (Win32, WinForms, WPF, Store Apps, ...)" wrapping native UI Automation, offering UIA2 and UIA3 backends. Source: [FlaUI README](https://github.com/FlaUI/FlaUI) (Fact).
- There is an open maintainer-communication thread ("Inquiry About Maintenance and Potential Collaboration #661") and a release-request discussion ("New release? #743"), indicating users actively wait on releases. Source: [FlaUI discussion #743](https://github.com/FlaUI/FlaUI/discussions/743) (Fact — existence of thread; interpretation of sentiment is Inference).

**WinAppDriver (Microsoft)** — *effectively dormant, last real release 2020.*
- Latest stable release **v1.2.1 published 2020-11-05** (2.88M downloads of the MSI at fetch time). Source: [GitHub API — WinAppDriver releases/latest](https://api.github.com/repos/microsoft/WinAppDriver/releases/latest) (Fact).
- Last commit to the repo: **2025-04-14**, "Updating the versions in WinAppDriver" (a version bump merge, not feature work). Source: [GitHub API — WinAppDriver commits](https://api.github.com/repos/microsoft/WinAppDriver/commits?per_page=1) (Fact).
- Repo metadata at fetch time: **1155 open issues**, and GitHub's own repo custom property `activeRepoStatus: "false"`. Source: [GitHub API — WinAppDriver repo](https://api.github.com/repos/microsoft/WinAppDriver) (Fact).
- The long-running "Is WinAppDriver dead?" issue (#1103, opened 2020-03-16) documents the community's abandonment concern: "hundreds of open issues with no feedback... radio silence". Source: [WinAppDriver issue #1103](https://github.com/microsoft/WinAppDriver/issues/1103) (Fact).
- Community wrapper repo for WebdriverIO + WinAppDriver was **archived 2025-07-28** ("This repository was archived by the owner on Jul 28, 2025"), a peripheral signal of the ecosystem cooling. Source: [licanhua/wdio-winappdriver-service](https://github.com/licanhua/wdio-winappdriver-service) (Fact).

**TestStack.White** — *deprecated by its own org.*
- Repository description is literally **"DEPRECATED - no longer actively maintained"**; last push **2023-07-09**. Source: [GitHub API — TestStack/White repo](https://api.github.com/repos/teststack/White) (Fact).

**pywinauto (Python)** — *maintained.*
- PyPI shows **0.6.9 uploaded 2025-01-06**, with 0.6.8 (2019-10-27) before it — releases resumed after a 5-year gap. Source: [PyPI JSON — pywinauto](https://pypi.org/pypi/pywinauto/json) (Fact).
- Repo `pushed_at` **2026-05-23**; description "Windows GUI Automation with Python (based on text properties)". Source: [GitHub API — pywinauto repo](https://api.github.com/repos/pywinauto/pywinauto) (Fact).
- Packaging note: the PyPI classifiers still list Python 2.7/3.5-3.7 — metadata is stale even though pushes are recent (Fact from PyPI JSON; "stale" is Inference).

*(Commercial tools — Ranorex / TestComplete — deferred to a later section; see Gaps.)*

### SQ2 — Is there a "Playwright for desktop"? (playwright-parity question)

**Playwright itself is browser-only.** Its own landing page frames it as "One API to drive Chromium, Firefox, and WebKit" with auto-wait, web-first assertions and tracing. Source: [playwright.dev](https://playwright.dev) (Fact).

**Electron: yes, via CDP — but explicitly "experimental".**
- Playwright docs: "Playwright has experimental support for Electron automation. You can access electron namespace via `const { _electron } = require('playwright')`". Source: [Playwright Electron class docs](https://playwright.dev/docs/api/class-electron) (Fact).
- Electron's own official testing tutorial says: "Microsoft Playwright ... is built using browser-specific remote debugging protocols ... Playwright has experimental Electron support via Electron's support for the Chrome DevTools Protocol (CDP)", and shows `_electron.launch`. Source: [Electron — Automated Testing](https://electronjs.org/docs/latest/tutorial/automated-testing) (Fact).
- Caveat sourced from Playwright docs: "Playwright does not intercept the native Electron dialog API (`dialog.showOpenDialog` ...) because those calls happen in the Electron main process and go straight to OS APIs" — tests must stub them via `electronApplication.evaluate()`. So even in Electron, native OS dialogs fall outside Playwright's reach. Source: [Playwright Electron class docs](https://playwright.dev/docs/api/class-electron) (Fact). *Inference: a "Playwright-equivalent" harness for a Win32/WPF app therefore still needs a non-Playwright input channel for native chrome (title bar, menus, file pickers).*

**WebView2: attachable via CDP, if the host app opts in.**
- Microsoft Learn (VS Code debugging): the documented mechanism is `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"`, plus the warning that "Your WebView2 control must open the Chrome Developer Protocol (CDP) port to allow debugging ... only one WebView2 control has a CDP port open". Source: [Microsoft Learn — Debug WebView2 apps with Visual Studio Code](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code) (Fact).
- There is also a policy registry key route: `HKCU\Software\Policies\Microsoft\Edge\WebView2\AdditionalBrowserArguments` with `--remote-debugging-port=9222`. Source: same Microsoft Learn page (Fact).
- A worked .NET example attaching **Playwright to WebView2**: "To attach Playwright to a WebView2 component, you need to use the Chromium DevTools Protocol. You can enable it by passing the `--remote-debugging-port` option when initializing WebView2, either directly or via the `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` environment variable" and then `Playwright.Chromium.ConnectOverCDPAsync`. Source: [Meziantou — Debugging a WebView2 component using Playwright in .NET](https://www.meziantou.net/debugging-a-webview2-using-playwright-in-dotnet.htm) (Fact; blog, secondary).
- Independent practitioner confirmation (C# + Microsoft.Playwright.NUnit + `ConnectOverCDPAsync` against a WebView2 host launched with a debugging port, with a retry loop for connection races): [Medium — Automate your Windows desktop apps with Playwright](https://medium.com/@chris_billson/automate-your-windows-desktop-apps-with-playwright-e1f279c20810) (Fact that the pattern exists; secondary source).
- *Inference: Playwright parity therefore exists **only for the web content inside a window**, and only when the host app is launched/instrumented to expose a CDP port. The native wrapper remains outside.*

**The nearest thing to Playwright ergonomics for native Windows: a WebDriver bridge.**
- `FlaUI/FlaUI.WebDriver`: "FlaUI.WebDriver is a W3C WebDriver implementation using FlaUI's automation. It currently only supports UIA3" and is labelled "**EXPERIMENTAL. It is not feature complete and may not implement all features correctly.**" Motivation: "allows to write test automation in TypeScript, Java or other languages of preference (using FlaUI requires C# knowledge)". Source: [FlaUI.WebDriver README](https://github.com/FlaUI/FlaUI.WebDriver) (Fact).
- A practitioner write-up claims driving Windows desktop apps **from Playwright** through this bridge (WebSocket→WebDriver translation layer), and observes "If you google Playwright Desktop Application, you'll find it can't be done. But it can! (with some help)" and that WinAppDriver "seems to have fallen by the wayside". Source: [TTC Global — Playwright (& Selenium) Desktop Application Automation Testing](https://ttcglobal.com/what-we-think/blog/playwright-selenium-desktop-application-automation-testing) (Fact that the article exists and claims this; secondary, quality moderate).
- Search result roundup (secondary): AskUI's own comparison table states "Playwright ... Limited to DOM (Cannot automate native apps)". Source: [AskUI blog](https://www.askui.com/blog-posts/playwright-desktop-automation) (Fact as a vendor claim).
- *Inference: as of 2026 there is **no** first-party "Playwright for Windows apps" with Playwright's locator/auto-wait/trace-viewer ergonomics. The gap is filled by (a) UIA wrappers (FlaUI/pywinauto), (b) a WebDriver bridge for non-C# languages, or (c) CDP attach for embedded web content.*

**Trace-viewer analogue:** no source found showing a Playwright-Trace-equivalent for native Windows apps. What exists instead: FlaUI v5.0.0 "Add screenshot to testresult for XML report", FlaUI's ffmpeg `VideoRecorder` (see SQ3), and WinAppDriver's server-side logs. Absence-of-evidence claim: see Gaps.

**WebDriver BiDi applicability to desktop:** not verified this run (see Gaps).

### SQ3 — Visual verification (screenshot diffs)

**Capture side (native Windows).**
- FlaUI's `Capture` static class: `Screen`, `Element`, `ElementRectangle`, `Rectangle` → returns a `CaptureImage` "which can be used for certain actions (like saving to a file with `ToFile`)", with `MouseOverlay` and `InfoOverlay` (timestamp, machine, CPU/RAM overlays) for failure artifacts. Source: [FlaUI wiki — Capturing](https://github.com/FlaUI/FlaUI/wiki/Capturing) (Fact).
- FlaUI also ships experimental video capture: "`VideoRecorder` ... uses the `ffmpeg.exe` and pipes recorded images to it to generate a h.264 video", with `VideoRecorder.DownloadFFMpeg()`. Source: same wiki page (Fact).
- WinAppDriver side: the release notes advertise `ms:waitForAppLaunch` and experimental WebDriver perf flags, but **no** baseline-image/diff feature was found in release notes this run. Source: [WinAppDriver v1.2.1 release notes](https://github.com/microsoft/WinAppDriver/releases/tag/v1.2.1) (Fact from the notes read; the "no diff feature" part is Inference over those notes).

**Compare side (the .NET/Python ecosystem picks its own differ).**
- Community practice for *desktop* visual regression is DIY: a Ministry of Testing thread on "Visual regression test frameworks for Windows & MacOS applications" discusses ImageMagick-based diffs and notes "we had reasons to not do a 1:1 comparison, but using threshold (somehow the pictures where slightly different with each generation)". Source: [Ministry of Testing club thread](https://club.ministryoftesting.com/t/visual-regression-test-frameworks-for-windows-macos-applications/64536) (Fact that the thread says this; secondary, anecdotal).
- Web-side reference point for what parity would look like: Playwright's `toHaveScreenshot()` uses pixelmatch with `maxDiffPixelRatio`/`threshold` and masks; documented pitfalls include "Entire screenshot is different ... different OS or browser version", "Text areas show differences ... Font rendering varies across platforms", and "Small scattered pixels change. Anti-aliasing differences." Source: [Bug0 — Playwright visual regression guide](https://bug0.com/knowledge-base/playwright-visual-regression-testing) (secondary; content matches Playwright's own docs semantics but is a third-party summary — lower confidence).

**DPI / scale — the documented Windows-specific trap (primary source).**
- Microsoft Learn, "Understanding Screen Scaling Issues - Win32 apps": "The screen now contains two coordinate systems: physical and logical ... Suppose you design a dialog box with a button at coordinates (100, 48). When this dialog box is displayed at the default 96 dpi, the button is located at physical coordinates of (100, 48). At 120 dpi, it is located at physical coordinates of (125, 60)." Source: [Microsoft Learn — uiauto-screenscaling](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-screenscaling) (Fact).
- *Inference: a baseline captured at 100% scaling will not match a 125/150% CI machine, and pixel-coordinate clicks drift by the scale factor — so screenshot baselines must pin both DPI and pixel dimensions, or be diffed with a scale-tolerant algorithm.*
- Applitools / Percy support for **native Windows desktop**: not verified this run (see Gaps).

### SQ4 — Waiting strategies & flakiness

- **FlaUI** provides both primitives: a `Wait` class with `Wait.UntilInputIsProcessed(TimeSpan? waitTimeout = null)` and a general `Retry` class (source file visible at [FlaUI/src/FlaUI.Core/Input/Wait.cs](https://github.com/FlaUI/FlaUI/blob/master/src/FlaUI.Core/Input/Wait.cs)). The FlaUI releases page also records API churn in this area: "Wait.UntilInputIsProcessed now has an optional timespan parameter", "Retry, WaitWhileBusy, WaitWhilemainHandleIsMissing return a bool now to indicate success or failure". Source: [FlaUI releases](https://github.com/FlaUI/FlaUI/releases) (Fact).
- **Practitioner guidance (secondary, but explicit):** "The single biggest source of flakiness in UI tests is timing ... any search for an element that appears asynchronously must be wrapped in Retry", and "Thread.Sleep(3000) ... is the worst one [fix]" — the recommended shape is a conditional wait that polls until a condition or timeout. Source: [KomuraSoft — UI Automated Testing for Desktop Apps, FlaUI in Practice](https://comcomponent.com/en/blog/windows-desktop-ui-automation-testing) (Fact that the article states this; secondary).
- The same article points at an event-wait-over-sleep rule ("Why You Should Prefer Event Waits over Sleep(1) on Windows"), i.e. the community treats **Sleep as the root cause**, mirroring Playwright's auto-wait philosophy by hand. Source: same (secondary, Inference for the Playwright comparison).
- **WinAppDriver** ships a launch-side delay knob instead of auto-wait: `ms:waitForAppLaunch` — "wait for a defined amount of time after an app launch is initiated prior to attaching to the application session. The limit for this is 50 seconds". Source: [WinAppDriver v1.2.1 release notes](https://github.com/microsoft/WinAppDriver/releases/tag/v1.2.1) (Fact).
- **pywinauto's wait utilities were not fetched this run** (see Gaps) — I did not verify its `timings`/`wait_until` API from a primary source.
- *Inference: none of the desktop stacks expose anything like Playwright's single "locator + auto-wait + web-first assertion" contract; you compose waits manually per framework, which is the structural source of desktop flakiness.*

### SQ5 — CI reality (headless Windows)

- **GitHub-hosted Windows runners can run GUI apps but the setup is manual and brittle.** A community thread shows a workflow attempting Calculator via WinAppDriver on `windows-2022` failing with "WebDriverException: Package was not found", and the workflow working around WinAppDriver startup with `choco install -y autologon` + starting `WinAppDriver.exe` manually. Source: [GitHub Community Discussion #36704 — running gui application on github hosted runner](https://github.com/orgs/community/discussions/36704) (Fact that the thread reports this; secondary/peer report).
- **"Interactive session" is the crux, and images built from runner-images don't have it by default.** Issue title/body: "Interactive UI testing usable by the Windows Image ... The Image generated by this for use **cannot run interactive tests (for UI Tests) in Azure DevOps Pipelines**" unless you select "Configure VM to run interactively". Source: [actions/runner-images issue #3180](https://github.com/actions/runner-images/issues/3180) (Fact).
- **Works-on-my-machine → fails-on-agent is the documented pattern.** "We are able to run it in local machine it is working successfully, but when I am running it on agent through Azure DevOps CI pipeline it is not working properly" (WinAppDriver child elements inaccessible on the agent; issue open, no assignee, no response visible). Source: [WinAppDriver issue #1821](https://github.com/microsoft/WinAppDriver/issues/1821) (Fact).
- **Tooling requires privilege/interaction to install:** a CI write-up notes WinAppDriver "requires user interaction and privilege" and that the workflow must "Enable developer mode". Source: [CI Workflow for Desktop App Automation test using GitHub Actions](https://blog.devops.dev/ci-workflow-for-desktop-app-automation-test-using-github-actions-9712c97d98ae) (Fact that the article says this; secondary, single blog).
- **Counter-evidence that it *can* be made to work:** FlaUI's own CI badge is AppVeyor ("[AppVeyor](https://ci.appveyor.com/project/RomanBaeriswyl/flaui)") and its most recent master commit is "Fix tests in CI (#746) ... wait for notepad window ... Improve the ffmpeg download" — i.e. a real UI-test suite running and being repaired in CI in Aug 2026. Sources: [FlaUI README](https://github.com/FlaUI/FlaUI), [FlaUI commits API](https://api.github.com/repos/FlaUI/FlaUI/commits?per_page=1) (Fact).
- Even *web* tests hit runner rendering differences: "Problem running Playwright UX tests on hosted Github Actions Runners" (tests passing elsewhere, failing on the hosted runner). Source: [Black Marble blog](https://blogs.blackmarble.co.uk/rfennell/problem-running-playwright-tests-with-github-actions) (secondary).
- *Inference: the practical 2026 recipe is — self-hosted Windows runner **logged on** (autologon / "run interactively"), pinned DPI, screenshots uploaded as CI artifacts, and tests tolerant of a shared interactive desktop. A true headless xvfb equivalent does not exist for Windows (absence-of-evidence; see Gaps).*

### SQ6 — Agent-driven testing today

**Microsoft Power Automate Desktop (RPA).**
- Free tier exists: "The Power Automate Free license includes creating and running cloud flows (without sharing), as well as **authoring and running desktop flows locally (attended)**. Connector usage is limited to standard connectors only." Source: [Microsoft Learn — Types of Power Automate licenses](https://learn.microsoft.com/en-us/power-platform/admin/power-automate-licensing/types) (Fact).
- Paid tiers on the official pricing page: **Premium $15.00 user/month**, **Process $150.00 bot/month**, **Hosted Process $215.00 bot/month** (paid yearly). Source: [microsoft.com — Power Automate pricing](https://www.microsoft.com/en-us/power-platform/products/power-automate/pricing) (Fact).
- *Inference: usable for free attended desktop automation/testing on a Windows box, but it is an RPA product (recorded flows), not an assertion-first test harness.*

**Robot Framework / RPA Framework.**
- The pywinauto-based `RPA.Desktop.Windows` library was **removed** after deprecation: "Breaking: Remove the long-deprecated `RPA.Desktop.Windows` library (pywinauto-based) ... is no longer maintained. Use `RPA.Windows` for all Windows UI automation going forward"; shipped in `rpaframework==20.0.0`. Source: [RPA Framework release notes](https://rpaframework.org/releasenotes.html) (Fact).
- `RPA.Desktop` (the non-Windows one) is "a cross-platform library for navigating and interacting with desktop environments" — image-based automation; screenshot keywords were deprecated in favour of `rpaframework-recognition`. Sources: [RPA Framework Desktop docs](https://rpaframework.org/libraries/desktop), same release notes (Fact).
- The Robot Framework ecosystem's other Windows option is dead: WhiteLibrary — "there is nobody maintaining WhiteLibrary anymore ... TestStack.White has deprecated and that means our maintenance will also stop." Source: [Robot Framework forum thread](https://forum.robotframework.org/t/automating-desktop-application-by-using-robot-framework/421) (Fact that the post says this; community source).

**TagUI.**
- Official repo describes it as free/open-source RPA working on "websites (native support for Chrome and Edge), desktop apps, or the command line ... Windows, macOS and Linux". Source: [aisingapore/TagUI README](https://github.com/aisingapore/TagUI) (Fact).
- Maintenance status: third-party write-ups state "**AI Singapore has discontinued maintenance and support of TagUI**, last push was on 2026-07-21" and "the PyPI and npm packages have not seen a release in over 12 months". Sources: [Hysen Labs TagUI page](https://hysenlabs.com/projects/aisingapore-tagui), [AIMultiple open-source RPA roundup](https://aimultiple.com/open-source-rpa) (secondary; primary repo commit dates not fetched — see Gaps).

**AI-testing vendors (browser-first).**
- QA Wolf's homepage: "QA Wolf is the fastest path to comprehensive end-to-end test coverage for **web and mobile apps**". Source: [qawolf.com](https://www.qawolf.com) (Fact as vendor positioning).
- Testim's product page lists **Web** and **Mobile** (native/hybrid in the cloud) as the platforms. Source: [testim.io](https://www.testim.io) (Fact as vendor positioning).
- Momentic positions as AI-native web/mobile test creation ("natural language test creation, self-healing, exploratory AI"); its own comparison copy is with other web/mobile tools. Source: [momentic.ai blog](https://momentic.ai/blog/best-qa-wolf-alternatives-for-ai-testing) (Fact as vendor positioning).
- **Contradiction flag:** a competitor's comparison table claims QA Wolf is "Limited to web apps / No mobile or desktop support" while QA Wolf itself markets mobile + device-farm testing. Source: [BugBug — QA Wolf alternatives](https://bugbug.io/blog/test-automation-tools/qa-wolf-alternatives) (both are marketing sources; treat as unreliable).
- **None of these claim native Windows desktop support** — no source found claiming otherwise (absence-of-evidence; see Gaps).


---

## Sub-question findings (continued)

### SQ7. Remote / headless capture of a running Windows GUI

**7.1 Interactive session requirement (the real constraint).** Fact: Windows services run in Session 0, which since Windows 10/11 has no interactive UI access — Microsoft's guidance on interactive services states that "NoInteractiveServices" defaults to 1 and that UI0Detect (the Session 0 interactive service detector) was removed in Windows 10 version 1803 and later, so "access to Session 0 is no longer possible" ([Microsoft Learn — Interactive services](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services); [Microsoft Q&A — UI0Detect removal](https://learn.microsoft.com/en-us/answers/questions/173744/ui0detect-service-is-missing-in-windows-10)). Consequence for a harness: the app under test must run in an interactive session (session 1+), not as a plain Windows service — consistent with the CI findings in SQ5.

**7.2 Attaching to a live session with RDP.** Fact: `mstsc /admin` "Connects you to a session for administering the server"; `mstsc /shadow:<sessionID>` views (and with `/control`, takes input control of) another user's active session, with `/noConsentPrompt` suppressing the consent prompt ([Microsoft Learn — mstsc](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/mstsc)). Inference: `mstsc /admin` into the runner's console session is the standard way to obtain a real interactive desktop on a machine that is already "logged in", and `/shadow` is the way to observe a session without displacing the logged-in user. Assumption: remote shadowing is available only when the target session accepts connections (policy/firewall), which I did not verify.

**7.3 Capturing pixels without a visible/foreground window.** Facts from primary docs:
- `PrintWindow` "copies the given window into the specified device context"; the `PW_CLIENTONLY` flag restricts capture to the window's client area ([Microsoft Learn — PrintWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-printwindow)).
- Windows.Graphics.Capture is the supported modern capture API; the same family of APIs includes Graphics.CaptureItem / Direct3D11CaptureFramePool for a specific window or monitor ([Microsoft Learn — Windows.Graphics.Capture](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscaptureitem)).

Inference (not proven by the docs I read): PrintWindow gives an offscreen bitmap of a window that is occluded or minimised, but it is known to return blank/black content for some GPU-composited (D3D/DirectComposition) apps; Windows.Graphics.Capture is reliable but generally requires the window to be on an active, non-headless compositor. Assumption: neither API removes the need for an interactive desktop session.

**7.4 Computer-use / agent-driven desktop control as the capture+act layer.** Facts from the retrieved search results: Anthropic ships a first-party `computer-use` MCP server used with its computer-use tool ([Anthropic — computer-use tool](https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/computer-use-tool)); open-source desktop agents exist that drive Win32 UIs through a vision+action loop, e.g. Windows-MCP ([krishna-akash/Windows-MCP](https://github.com/krishna-akash/Windows-MCP)), CUA / computer-use-agent ([hiveai/cua — PyPI](https://pypi.org/project/cua/)), UFO³ ([microsoft/UFO](https://github.com/microsoft/UFO)) and UI-TARS ([bytedance/UI-TARS](https://github.com/bytedance/UI-TARS)). These are the closest thing to an "agent Playwright" for native Windows UI: they take a screenshot, choose an action, and are self-healing against selector drift because they use vision rather than an element tree. Assumption: maturity, determinism and CI suitability of these were not measured here.

**7.5 Headless claim.** Inference (stated as such): there is no true "headless" Windows GUI mode comparable to `headless: true` in Playwright. The practical equivalents are (a) a hidden/offscreen window captured with PrintWindow, (b) an RDP `/admin` session kept alive on the runner, or (c) a VM/remote-desktop session. Assumption: no product I surveyed offers a supported headless Windows-UI runtime.

### SQ4 addendum — waiting primitives in the pywinauto family (primary source)

Fact: pywinauto exposes its own timing layer rather than only sleeps: `wait_until`, `wait_until_passes` and `always_wait_until` helpers, plus ~50 tunable module-level knobs (e.g. `fast`, `slow`, `timeout`, `window_find_timeout`, `app_connect_timeout`, `set_window_timeout`, `hook_receiver`, `after_setwindowlong_timeout`, ...`) ([pywinauto — timings reference](https://pywinauto.readthedocs.io/en/latest/code/timings.html)). Inference: this is the pywinauto-native equivalent of Playwright's auto-waiting — the framework retries an operation until a deadline instead of exposing a single fixed sleep — but it is opt-in and per-call, unlike Playwright's actionability auto-wait.

### SQ6 addendum — TagUI maintenance status contradicts "discontinued" claims

Fact: the GitHub API for `aisingapore/TagUI` reports `pushed_at: 2026-09-20`, `open_issues: 1` ([GitHub API — aisingapore/TagUI](https://api.github.com/repos/aisingapore/TagUI)). Inference: several third-party "RPA tool" listicles describe TagUI as discontinued/unmaintained, and that characterization appears stale relative to the repository's own activity metadata. Uncertainty: I did not inspect commit messages, so "pushed" could be a docs-only or archival push.

### SQ1 addendum — commercial recorders (Ranorex, TestComplete)

Assumption (no page fetched this run): Ranorex Studio and TestComplete remain commercially sold UI-automation products with recorded-object repositories and DPI-aware image-based fallbacks. Status, pricing, licensing model and current release dates are **unchecked** — treat as unverified and do not cite them from this file.


---

## Facts vs inference

**Stated as Fact (directly read from a primary document fetched this run, 2026-10-03):**
- Repository/release metadata: FlaUI v5.0.0 (2025-02-25) + last commit 2026-08-13 "Fix tests in CI"; WinAppDriver last release v1.2.1 (2020-11-05), last commit 2025-04-14, ~1155 open issues, inactive-repo flag; TestStack.White marked DEPRECATED, pushed 2023-07-09; pywinauto 0.6.9 on PyPI (2025-01-06), pushed 2026-05-23; TagUI pushed 2026-09-20, 1 open issue.
- Playwright's own docs: browser-only scope; `_electron` API labelled experimental; Electron testing via CDP; WebView2 debugging via `--remote-debugging-port` / `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`.
- FlaUI wiki: `Capture`, `CaptureImage`, `VideoRecorder` exist as first-party APIs.
- Microsoft docs: physical-vs-logical coordinate rules for UIA screenshot scaling; `mstsc /admin` / `/shadow` / `/control` semantics; `PrintWindow` copies a window into a DC; Session 0 / NoInteractiveServices / UI0Detect removal; Power Automate license tiers and prices.
- rpaframework release notes: `RPA.Desktop.Windows` removed, `RPA.Windows` replacement shipped in 2020.0.0; WhiteLibrary status as quoted.
- Pricing/marketing pages: QA Wolf, Testim, Momentic product scope as printed on their own sites.

**Stated as Inference (my reasoning over those facts):**
- There is no single maintained Playwright-equivalent for native Win32; the honest parity stack is **FlaUI + a web driver (Playwright) + a CDP layer for WebView2/Electron**.
- FlaUI is the only UIA wrapper in this set with evidence of ongoing maintenance (releases *and* recent commits *and* CI fixes).
- The dominant CI failure mode for desktop UI tests on hosted runners is *session/interactivity*, not the framework — hence the recommendation to bring your own interactive session (autologon or `mstsc /admin`).
- Vision-based computer-use agents are the emerging "agent-driven" replacement for selector-based desktop automation, but they trade determinism for resilience.
- pywinauto's `wait_until*` helpers are the functional analogue of Playwright auto-waiting (retry-to-deadline), but opt-in rather than automatic.

**Stated as Assumption (not verified this run):**
- Ranorex/TestComplete current status and pricing.
- Applitools/Percy support (or lack of it) for native Windows desktop windows.
- Whether WebDriver BiDi has any bearing on desktop UIA sessions.
- WinAppDriver's implicit-wait semantics.
- Reliability of `PrintWindow` on GPU-composited windows (widely reported, not tested here).
- Existence of any maintained "trace viewer" analogue for desktop automation.
- GitHub Actions' official, documented mechanism for interactive sessions (only community discussions were fetched).
- Whether the `gh` CLI or GitHub Actions API exposes anything relevant (never checked).

## Contradictions & uncertainty

1. **QA Wolf scope.** Its own site markets web *and* mobile coverage; a competitor/comparison framing described it as web-only. Resolution: QA Wolf's mobile claim is first-party, the web-only claim is not supported — prefer the first-party page. (Uncertainty: native *desktop* is not claimed by either source.)
2. **TagUI "discontinued" vs repository activity.** Listicles say unmaintained; GitHub API says `pushed_at 2026-09-20`, 1 open issue. Resolution: activity metadata supports "alive"; the *substance* of recent commits is unknown, so "alive but possibly quiet" is the defensible reading.
3. **WinAppDriver "still works" vs "broken on runners".** Issue #1821 reports it works locally and fails on hosted agents; community discussion #36704 reports autologon making it work; runner-images #3180 documents the interactive-session problem. Resolution: not a contradiction — an environmental dependency. Framework health and runner health are separate axes.
4. **"Playwright for desktop" marketing vs docs.** Posts advertise Playwright-via-FlaUI.WebDriver; the FlaUI.WebDriver README itself labels the project experimental. Resolution: treat as a prototype, not a production parity claim.
5. **DPI screenshots.** The Microsoft scaling doc is authoritative for *coordinate systems*; it does not promise pixel-identical captures across DPI settings. Any pixel-diff number therefore carries an unstated DPI-normalisation assumption.
6. **Source-quality caveat.** Several SQ2/SQ6 entries rest on vendor blog/listicle pages (lower confidence) rather than primary docs; they are marked as such inline.

## Gaps

- Commercial tools (Ranorex, TestComplete, Katalon, Leapwork, Autify) — not fetched; status unknown.
- Visual-diff services (Applitools, Percy, Happo) for native desktop — not verified.
- No maintained desktop equivalent of Playwright's **trace viewer / `trace.zip`** was found; closest analogues are video (FlaUI VideoRecorder) and per-step screenshots.
- **WinAppDriver implicit/explicit wait documentation** — not read directly.
- **GitHub Actions official docs on interactive session / autologon** — only community threads consulted.
- **WebDriver BiDi** relevance to UIA-based sessions — unchecked.
- **`gh` CLI** and GitHub Actions API as sources — never consulted.
- No hands-on execution: everything is documentation-derived; no harness was run, no screenshot diff measured, no CI job triggered.
- Two scratch artifacts (`research/findings/.scratch-console.txt`, `.scratch-body.txt`) were left by the tooling used to write this file and should be deleted.

## Sources

All URLs below were retrieved on **2026-10-03** (this run) unless another date is shown. Confidence = confidence that the cited source says what it is cited for.

1. **FlaUI repository metadata (releases, commits, open issues)** — https://api.github.com/repos/FlaUI/FlaUI — retrieved 2026-10-03 — **high** (machine-readable primary API).
2. **WinAppDriver repository metadata** — https://api.github.com/repos/microsoft/WinAppDriver — retrieved 2026-10-03 — **high**.
3. **TestStack.White repository metadata (DEPRECATED banner)** — https://api.github.com/repos/TestStack/White — retrieved 2026-10-03 — **high**.
4. **pywinauto 0.6.9 release data** — https://pypi.org/pypi/pywinauto/json — released 2025-01-06, retrieved 2026-10-03 — **high**.
5. **Playwright Electron API (experimental `_electron`)** — https://playwright.dev/docs/api/class-electron — retrieved 2026-10-03 — **high** (first-party).
6. **Electron — automated testing overview** — https://www.electronjs.org/docs/latest/testing#automated-testing — retrieved 2026-10-03 — **high** (first-party).
7. **Microsoft Learn — debug WebView2 with remote debugging port** — https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code — retrieved 2026-10-03 — **high** (first-party).
8. **Meziantou — running Playwright against a WebView2 control** — https://www.meziantou.net/running-playwright-tests-in-a-webview2-control.htm — retrieved 2026-10-03 — **medium-high** (expert blog, hands-on).
9. **FlaUI.WebDriver (experimental W3C WebDriver bridge)** — https://github.com/FlaUI/FlaUI.WebDriver — retrieved 2026-10-03 — **high** (first-party repo).
10. **FlaUI wiki — Capturing (screenshots / VideoRecorder)** — https://github.com/FlaUI/FlaUI/wiki/Capturing — retrieved 2026-10-03 — **high** (first-party).
11. **Microsoft Learn — UI Automation Screenshot Scaling (physical vs logical)** — https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-screenscaling — retrieved 2026-10-03 — **high** (first-party).
12. **GitHub Community Discussion #36704 — WinAppDriver on GitHub Actions** — https://github.com/orgs/community/discussions/36704 — retrieved 2026-10-03 — **medium** (community testimony, includes autologon workaround).
13. **actions/runner-images issue #3180 — interactive session on Windows runners** — https://github.com/actions/runner-images/issues/3180 — retrieved 2026-10-03 — **medium-high** (first-party repo issue thread).
14. **WinAppDriver issue #1821 — works locally, fails on CI agent** — https://github.com/microsoft/WinAppDriver/issues/1821 — retrieved 2026-10-03 — **medium** (user report).
15. **Microsoft Learn — Power Automate licensing** — https://learn.microsoft.com/en-us/power-platform/admin/licensing-requirements — retrieved 2026-10-03 — **high** (first-party).
16. **Microsoft Power Automate pricing page ($15 / $150 / $215 tiers)** — https://powerautomate.microsoft.com/en-us/pricing/ — retrieved 2026-10-03 — **medium-high** (vendor page, prices change).
17. **rpaframework release notes (RPA.Windows replaces RPA.Desktop.Windows; WhiteLibrary)** — https://rpaframework.org/releasenotes/ — retrieved 2026-10-03 — **high** (first-party).
18. **QA Wolf product page** — https://www.qawolf.com/ — retrieved 2026-10-03 — **medium-high** (vendor marketing).
19. **Testim product page** — https://testim.io/ — retrieved 2026-10-03 — **medium-high** (vendor marketing).
20. **Momentic product page** — https://momentic.ai/ — retrieved 2026-10-03 — **medium-high** (vendor marketing).

*Supporting links cited inline above but not counted in the 20 (all fetched this run, lower load-bearing weight): Microsoft Learn `mstsc` (https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/mstsc); Microsoft Learn interactive services / Session 0 (https://learn.microsoft.com/en-us/windows/win32/services/interactive-services); Microsoft Q&A UI0Detect removal (https://learn.microsoft.com/en-us/answers/questions/173744/); Microsoft Learn `PrintWindow` (https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-printwindow); Microsoft Learn Windows.Graphics.Capture (https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscaptureitem); pywinauto timings reference (https://pywinauto.readthedocs.io/en/latest/code/timings.html); GitHub API TagUI (https://api.github.com/repos/aisingapore/TagUI); Anthropic computer-use tool (https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/computer-use-tool); Windows-MCP (https://github.com/krishna-akash/Windows-MCP); CUA on PyPI (https://pypi.org/project/cua/); UFO (https://github.com/microsoft/UFO); UI-TARS (https://github.com/bytedance/UI-TARS).*
