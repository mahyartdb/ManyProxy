# 02 — Vision-based GUI agents vs. structured accessibility-tree automation

Research run: 2026-10-03. Epistemic rule: every claim below carries a source URL retrieved this run; anything without one is labeled.

## Sub-question findings

### 1. Grounding / screenshot→coordinates parsing models

- **Microsoft OmniParser exists and is actively maintained.** Repo `microsoft/OmniParser` (25.5k stars, 2.2k forks, 156 commits at fetch time), self-described as "A simple screen parsing tool towards pure vision based GUI agent." Paper: arXiv 2408.00203. ([S1], fetched 2026-10-03)
- **What it does:** "parses user interface screenshots into structured and easy-to-understand elements" so a VLM (they cite GPT-4V) can "generate actions that can be accurately grounded in the corresponding regions of the interface." Pipeline = interactive-region object detector (YOLO-family) + icon functional captioner + (implicitly) OCR/label rendering — the README explicitly ships an **YOLOv9-E interactive region detector** (news item 2026/7) and **icon caption models** ("icon_caption" weights, Florence-based directory `icon_caption_florence`). Earlier Ultralytics-based detectors are AGPL-licensed; `icon_detect_v3` is based on MIT-licensed YOLOv9. ([S1])
- **Version timeline (from repo News):** v1.5 (2024/11) added finer small-icon detection + interactability prediction; **v2 checkpoints released 2025/2**; v2 reported **39.5% on Screen Spot Pro** grounding benchmark at announcement (2025/1). **OmniTool** = "Control a Windows 11 VM with OmniParser + your vision model of choice" supporting OpenAI 4o/o1/o3-mini, DeepSeek R1, Qwen 2.5VL, Anthropic Computer Use (2025/2). ([S1])
- **OmniParser won on Windows Agent Arena:** repo news 2024/09 "OmniParser achieves the best performance on Windows Agent Arena" and WAA repo news 2024-10-23 calls it "the current top performing screen understanding model in our benchmark." ([S1], [S3])

- **Set-of-Mark (SoM) prompting exists** (arXiv 2310.11441, Oct 2023, microsoft/SoM). Method: "employ off-the-shelf interactive segmentation models, such as SEEM/SAM, to partition an image into regions… and overlay these regions with a set of marks e.g., alphanumerics, masks, boxes. Using the marked image as input, GPT-4V can answer the questions that require visual grounding." Claimed: GPT-4V+SoM zero-shot beat a fully-finetuned REF model on RefCOCOg. ([S4])
- **UI-TARS exists** (arXiv 2501.12326, Jan 2025, ByteDance): "a native GUI agent model that solely perceives the screenshots as input and performs human-like interactions" — end-to-end, no wrapper prompts. Reported: **OSWorld 24.6 @50 steps / 22.7 @15 steps** (vs Claude 22.0 / 14.9 in same paper); **AndroidWorld 46.6** vs GPT-4o 34.5. Key techniques: unified action modeling, "System-2 Reasoning," iterative training with reflective online traces. ([S5])
- **Windows Agent Arena (WAA)** = 150+ Windows tasks adapted from OSWorld; benchmark parallelized on Azure ("as little as 20 minutes"). Baseline agent **Navi: 19.5% success vs 74.5% unassisted human** (paper abstract, Sep 2024). ([S3])

- **Agent S / Agent S2 exists** — original: arXiv 2410.08164 "Agent S: An Open Agentic Framework that Uses Computers Like a Human" (Oct 2024): "Evaluation on the OSWorld benchmark shows that Agent S outperforms the baseline by **9.37% on success rate (an 83.6% relative improvement)**… demonstrates broad generalizability to different operating systems on a newly-released WindowsAgentArena benchmark." Successor: arXiv 2504.00906 "Agent S2… Mixture-of-Grounding… Proactive Hierarchical Planning" (Apr 2025); code `simular-ai/Agent-S` (12,504 stars, last push 2026-09-05 via `gh`). Agent S2 claims +18.9%/+32.7% over Claude Computer Use and UI-TARS on OSWorld 15-/50-step, "**+52.8% on WindowsAgentArena**", +16.52% on AndroidWorld (relative). ([S12])
- **ShowUI exists** — arXiv 2411.17465 (Nov 2024), `showlab/ShowUI` (1,910 stars): 2B VLA model, "**75.1% accuracy in zero-shot screenshot grounding**", UI-guided visual token selection cuts 33% redundant tokens, 1.4× speedup; frames the trade-off explicitly: prior agents "relying on closed-source API with text-rich meta-information (e.g., HTML or accessibility tree)… show limitations in perceiving UI visuals as humans do." ([S13])
- **Aria-UI exists** — arXiv 2412.16256 (ACL 2025): "Aria-UI adopts a **pure-vision approach, eschewing reliance on auxiliary inputs** [HTML/AXTree]… sets new state-of-the-art results across offline and online agent benchmarks, outperforming both vision-only and AXTree-reliant baselines." ([S14])
- **"SeeAction" is NOT an online GUI agent** — arXiv 2503.12873 (ICSE 2025): a CV model that reverse-engineers `[command][widget][location]` from screencasts for test/RPA scripting (11 commands, 11 widgets; 7,260 video-action pairs incl. Windows 10 Settings). ([S15])
- **ShowUI-Aloha (Jan 2026)** — arXiv 2601.07181, `showlab/ShowUI-Aloha`: pipeline turning human screen recordings into tasks; executor acts "at the OS level… with safety checks and real-time feedback"; README description says "for Real **Windows** and MacOS Desktops." ([S16])
- **ShowUI-π (Dec 2025)** — arXiv 2512.24965: documents a *capability* failure mode — discrete (x,y) click prediction "prohibits free-form, closed-loop trajectories (e.g. dragging a progress bar)"; new ScreenDrag benchmark: Operator 13.27, best Gemini-2.5-CUA 22.18, ShowUI-π 26.98 (450M params). ([S16])

### 2. Full desktop-agent frameworks operating Windows

- **Microsoft UFO → UFO² → UFO³** (repo `microsoft/UFO`). Timeline per repo: UFO Feb 2024 (GUI agent for Windows, arXiv 2402.07939), **UFO² Apr 2025** "Desktop AgentOS" (arXiv 2504.14603, now LTS), UFO³ "Galaxy" Nov 2025 multi-device orchestration (arXiv 2511.11332). ([S2])
- **UFO² is the hybrid case (UIA + screenshots):** repo lists "**Deep OS Integration** — Windows UIA, Win32, WinCOM native control", "**Hybrid GUI + API actions**", "**Visual + UIA Detection** — Hybrid control detection for robustness", "Speculative Multi-Action — Batch predictions → 51% fewer LLM calls", and a RAG "Knowledge Substrate". Config default `VISUAL_MODE: True` with GPT-4o. ([S2])
- **UFO³ scope:** multi-device DAG orchestration over Windows, Linux, Android ("more coming"); UFO² remains the Windows device agent. ([S2])
- **OS-Copilot / FRIDAY** (arXiv 2402.07456): open-source library "to build generalist agents capable of automatically interfacing with comprehensive elements in an operating system (OS)"; vision mode added 2024.9 (`friday_vision`), still marked "under development". README's overview text (commented out) scopes it to "Linux and MacOS" — Windows support not claimed in active README text. ([S11])
- **FLADE:** not found — arXiv full-text search `all:"FLADE" AND (GUI | computer agent)` = 0 results, `gh search repos "FLADE"` = unrelated projects only; treated as **unverified belief** (see Gaps). ([S18])

### 3. Evaluator / verification + benchmarks with reported success rates

- **OSWorld** (arXiv 2404.07972): 369 tasks, supports Ubuntu/Windows/macOS. Abstract: "humans can accomplish over 72.36% of the tasks, the best model achieves only **12.24%** success, primarily struggling with **GUI grounding and operational knowledge**." ([S6])
- **OSWorld-Verified** (repo update 2025-07-28): benchmark fixes + re-run model results published on os-world.github.io; users told to compare against new numbers. ([S6])
- **Verification mechanism (OSWorld):** each task "includes a detailed initial state setup configuration and a custom **execution-based evaluation script** for reliable, reproducible evaluation" — scripted state assertions for scoring, not VLM judgment. ([S6])
- **WAA cost/latency of a full vision-agent run** (README FAQ): GPT-4o ≈ **$100** API + ~$8 Azure VM, ~35 min with 40 VMs; GPT-4o-mini ≈ $15. ([S7])
- **Playwright MCP determinism stance:** "Deterministic tool application — avoids ambiguity common with screenshot-based approaches"; uses "accessibility tree, not pixel-based input"; screenshot tool: "You can't perform actions based on the screenshot, use browser_snapshot for actions." ([S8])

### 4. MCP / installable computer-use tooling today

- **`playwright-mcp` (microsoft/playwright-mcp) — browser-only.** "MCP server that provides browser automation capabilities using Playwright… through structured accessibility snapshots, bypassing the need for screenshots or visually-tuned models." Opt-in `--caps vision` adds "coordinate-based interactions" — still inside the browser; it does not operate native Windows apps. Runs fine on Windows (profile path `%USERPROFILE%\AppData\Local\ms-playwright\...`). ([S8])
- **cua / cua-driver (trycua/cua) — Windows supported.** "Give your agent tools to inspect and operate native desktop apps and browsers on macOS, **Windows**, and Linux. Connect through the CLI, **MCP**, or typed SDKs." Windows install: `irm https://cua.ai/driver/install.ps1 | iex`. "Background delivery lets agents work without moving your pointer or taking focus when the app and platform support it." Packages: `cua-driver`, `cua` SDK (Python/TS/Swift/Kotlin), `cua-spacesd` daemon port 3211 (gRPC streaming), `cua-bench`, CUA-S1 decision models; optional `cua-perception` extension bundles OmniParser (AGPL) + PP-OCR. ([S9])
- **Anthropic computer-use reference** (`anthropics/anthropic-quickstarts` computer-use-demo) exists: Docker image with **Linux desktop (X11 + VNC)**, agent loop, Anthropic `computer` tools, Streamlit UI; newest toolset `computer_toolset_20260801` = one member tool per action (`left_click`, `screenshot`, …). **It is a Linux-in-Docker reference, not a Windows driver.** ([S10])
- **Microsoft `windows-agent-adapter`: could not verify existence.** `gh search repos "windows-agent-adapter"` returned only unrelated MTConnect/adapter repos; two web searches found nothing. Negative result — see Gaps. ([S18])

#### Windows-specific MCP servers found today (GitHub search, 2026-10-03; README-level verification)

| Package | Stars | Windows support claim (repo description / README) |
|---|---|---|
| `Harusame64/desktop-touch-mcp` | 21 (pushed 2026-10-03) | **Windows 10/11 only**; "semantic discover-then-act targeting (entities + leases, **not pixel coordinates**)… native Rust UIA engine, Chrome CDP". README: 32 tools, `npx -y @harusame64/desktop-touch-mcp`. ([S17]) |
| `zavora-ai/computer-use-mcp` | 72 (pushed 2026-10-01) | "High Performance MCP server + client for **Windows and macOS** computer control built in Rust. Screenshot, mouse, keyboard, clipboard, app management." (description only, README not read) ([S18]) |
| `sandraschi/windows-computer-use-mcp` | 40 (pushed 2026-10-02) | "Windows Computer Use… 22 MCP tools for click, type, screenshot, OCR, UI inspection." (description only) ([S18]) |
| `bigduu/Nova` | 22 (pushed 2026-10-03) | "Cross-platform Rust computer-use MCP server for macOS and Windows: **AX/UIA**, screenshots, OCR, input." (description only) ([S18]) |
| `domdomegg/computer-use-mcp` | 381 (pushed 2026-09-09, branch `master`) | "Give AI models complete control of your computer (probably a bad idea)". README body not readable via raw fetch (404 on `main`, gh api readme truncated) → platform support **unverified**. ([S18]) |
| `regulad/vnc-mcp` | 13 | "allows MCP-compatible LLMs to interact with any desktop accessible over **VNC**" — the desktop-streaming category exists but is tiny. (description only) ([S18]) |

- **SMB-based desktop-streaming MCP:** no repository matching "SMB" streaming of desktops was found in GitHub search; VNC/frame-streaming MCP servers exist but are low-activity (top hit 13 stars). (Negative result.) ([S18])

### 5. Documented failure modes (each with source)

- **Imprecise/hallucinated coordinates.** WAA paper: "we commonly find that **imprecise SoM bounding box boundaries lead to incorrect element selection**"; and agents "correctly output the action textual description (`move to the red color`) but fail to select the visual ID (#59 corresponds to the yellow palette)" — blamed on VLM training-distribution mismatch with "hundreds of small-scale markers". ([S3])
- **Vision-only vs structured — quantified gap.** WAA Table 4 (GPT-4V-1106): pixel-only OmniParser SoM = **12.5%** total; UIA+OmniParser = **19.5%**; pure open-source detector stack (Pytesseract+GroundingDINO) = 8.6%. "Adding high-quality UIA markers… boosts performance by **57% for Omniparser**, 52% for open-sourced models, and 15% for proprietary pixel models." ([S3])
- **Structured tree's own cost/limits.** Same passage: "Using UIA markers, however, can take **from a few seconds up to several minutes** to be queried depending on the complexity of the screen." WAA README: `--a11y-backend win32` = "faster, less accurate accessibility tree"; `uia` = "slower, more accurate". ([S3],[S7])
- **Screenshot scaling / DPI.** Anthropic reference: "We do not recommend sending screenshots in resolutions above XGA/WXGA to avoid issues related to image resizing… Relying on the image resizing behavior in the API will result in lower model accuracy and slower performance"; recommends scaling down to 1024×768 and "map the coordinates back to the original resolution proportionally", padding smaller screens with black. ([S10])
- **Multi-monitor + stale coordinates + wrong-window input** (Windows MCP engineering log): `desktop-touch-mcp` README enumerates `coordinate_outside_reachable_bounds` ("monitors placed left of or above the primary have **negative desktop coordinates**"), `stale-coordinate clicks`, `cursor_placement_blocked`, `keyboard_target_unsafe` (wrong focus/disabled/read-only field), "mouse homing correction" for windows that move between screenshot and click, and a `dotByDot`/`origin`+`scale` mode to "eliminate off-by-one / scale bugs". Treat as *self-reported by a 21-star community project*, but it documents exactly the failure classes asked about. ([S17])
- **Timing/flicker & environment nondeterminism.** OSWorld-Verified post-mortem: "Many tasks exhibit complex temporal dependencies… loading and response times, creating timing-sensitive scenarios"; fixed by converting fire-and-forget to blocking ops and "iteratively adjusted inter-action delays"; also CAPTCHA/anti-bot blocks, DOM churn, and "providing reliable rewards consumes more human resources than we imagined". ([S19])
- **Benchmark-signal reliability / replay.** OSWorld-Verified: 300+ issues fixed; decentralized evaluation led to "everyone… modifying tasks for evaluation to achieve higher scores, making scores increasingly opaque and incomparable"; a public centralized evaluation platform now required for verified leaderboard entries. ([S19])
- **VLM cost/latency.** WAA FAQ: a full 150-task run with GPT-4o ≈ **$100** in API charges (+ ~$8 Azure VM, ~35 min on 40 VMs); GPT-4o-mini ≈ $15. ([S7])
- **Hallucination in planning.** WAA: "Phi3-V tends to hallucinate when planning multiple steps ahead or presented with long input contexts." ([S3])
- **Non-determinism/variance of agents.** UI-Evol (arXiv 2505.21964) notes "a previously overlooked issue of **high behavioral standard deviation in computer use agents**" and "even 90% correct knowledge yields only 41% execution success rate." ([S20])
- **Silent action failures.** Minitap (arXiv 2602.07787) failure analysis on AndroidWorld: "context pollution from mixed reasoning traces, **silent text input failures undetected by the agent**, and repetitive action loops without escape"; its "deterministic post-validation of text input against device state" added +7 points. ([S20])

### Reliability: vision vs structured — headline numbers

| Benchmark (source) | Date | Human | Vision-only agents | Best agent (often hybrid) |
|---|---|---|---|---|
| OSWorld 369 tasks ([S6]) | Apr 2024 | 72.36% | GPT-4V-era best 12.24% | — |
| WindowsAgentArena ([S3],[S7]) | Sep 2024 | 74.5% | OmniParser SoM + GPT-4V: 12.5% | UIA+OmniParser+GPT-4V: **19.5%** |
| AndroidWorld 116 tasks ([S20]) | 2024→2026 | 80% (per Minitap) | best baseline 30.6% at launch | Minitap 100% (2026, system-level verification) |
| OSWorld-Verified ([S19]) | Jul 2025 | ~72% | GPT-4o 5%; Claude 4 Sonnet 43.9% | CoACT-1 60.76%, Agent S2.5+o3 56.0% |


## Facts vs inference

- **Fact:** OmniParser = YOLO-family interactive region detector + icon captioner (Florence weights); `icon_detect_v3` MIT, earlier Ultralytics detectors AGPL. ([S1])
- **Fact:** WAA best config (UIA + OmniParser + GPT-4V-1106) = 19.5% vs human 74.5%; the same SoM without UIA = 12.5%. ([S3])
- **Fact:** UI-TARS is screenshot-only end-to-end; reported OSWorld 24.6 (50-step) / 22.7 (15-step) at publication. ([S5])
- **Fact:** UFO² exposes Windows UIA/Win32/WinCOM structured control *and* visual detection ("Hybrid control detection for robustness"). ([S2])
- **Fact:** Playwright MCP is browser-scoped and explicitly accessibility-tree-first; vision/coordinate tools are opt-in and browser-internal. ([S8])
- **Fact:** Anthropic's reference computer-use implementation targets Linux-in-Docker, not Windows; it documents image down-scaling to 1024×768 + coordinate remapping as required practice. ([S10])
- **Fact:** No GitHub repository or web result found for a Microsoft project named `windows-agent-adapter` (negative search result, 2026-10-03). ([S18])
- **Fact:** Package activity: `@playwright/mcp` 0.0.83 (npm, 2026-09-28); `@harusame64/desktop-touch-mcp` 2.0.0 (npm, 2026-09-24); `@trycua/cua` 0.2.0 (npm, 2026-10-01); PyPI `cua` 0.2.0 and `desktop-env` 1.0.2. ([S18])
- **Inference:** The WAA +15…57% lift from adding UIA markers, plus UFO²'s hybrid mode and the fact that every top OSWorld-Verified entry is a framework (CoACT-1, Agent S2.5, GTA1) rather than a bare vision model, suggests current SOTA on *Windows* is hybrid: structured state for grounding/verification, vision as fallback/filler. (Derived from [S2],[S3],[S19])
- **Inference:** Structured accessibility trees are not uniformly superior — WAA itself rates the Win32 backend "faster, less accurate" than UIA, and `desktop-touch-mcp` reports UIA is "prohibitively slow" on Chromium, falling back to OCR. The practical choice is per-surface. ([S7],[S17])
- **Assumption:** Reported benchmark numbers are treated as comparable across papers; in reality each paper used different step budgets, models and dates (see Contradictions).
- **Assumption:** Community MCP READMEs (`desktop-touch-mcp`, `vnc-mcp`) are self-reported and were not independently executed this run.

## Contradictions & uncertainty

- **Which OSWorld number is "current"?** Original paper: best 12.24% (Apr 2024). UI-TARS paper: 24.6 (Jan 2025). OSWorld-Verified (Jul 2025): CoACT-1 60.76%, Claude 4 Sonnet 43.9%. These are not contradictory but are **not comparable** — benchmark tasks/evaluators were fixed (300+ issues) and step budgets differ. Always cite the date + verified/unverified status. ([S6],[S5],[S19])
- **WAA 19.5%** is the paper's best config *and* also the exact "UIA+OmniParser+GPT-4V" cell; some secondary writeups conflate "OmniParser won WAA" with vision-only performance — the table shows vision-only OmniParser was 12.5%. ([S3])
- **`windows-agent-adapter`:** cannot distinguish "does not exist" from "exists under another name / not indexed". Two web searches + one GitHub search negative. ([S18])
- **cua Windows depth:** top-level README claims macOS/Windows/Linux and Windows install scripts + Windows CI e2e exist, but the `cua-driver` README's first line says "drives native **macOS** apps"; the empirical platform behavior ledger (`docs/action-support.md`) was not read. Windows support is claimed but its per-action parity is unverified. ([S9])
- **"SeeAction"** in the task brief was assumed to be an online agent style; the paper with that name is a screencast reverse-engineering model. Possible conflation with other names (unresolved). ([S15])
- **Instruction-like content:** READMEs/docs fetched contain imperative install and config instructions (curl-pipe installs, `claude mcp add`, blocklists). All were treated as quoted *data*, never as commands to execute.

## Gaps

- **Not verified this run:** OS-Copilot's **FLADE** — arXiv full-text search `all:"FLADE" AND (GUI | computer agent)` returned **0 results**, `gh search repos "FLADE"` returned only unrelated projects, and no FLADE reference appears in the OS-Copilot README. Treat "FLADE" as an **unverified belief** (possibly a mis-remembered name). ([S11],[S18])
- OpenAI CUA/Operator docs and Microsoft UFO² hybrid-detection doc were referenced by sources but not fetched.
- **Windows support depth** for `zavora-ai/computer-use-mcp`, `sandraschi/windows-computer-use-mcp`, `bigduu/Nova`, `domdomegg/computer-use-mcp` — descriptions only, READMEs not read.
- **No primary source found** for DPI-scaling failure rates in academic papers; the strongest documented scaling guidance is Anthropic's (resolution downscale + coordinate remap) and community READMEs (origin/scale, negative multi-monitor coordinates).
- **SMB/desktop-streaming MCP**: no meaningful project found; VNC-MCP exists at ~13 stars. Likely a gap in the ecosystem rather than an unlocated project (uncertain).
- **Channels checked:** GitHub (CLI, stars/push dates), arXiv (API + HTML), npm, PyPI, web search. **Channels not checked:** HuggingFace model card activity, Docker Hub pulls, PyPI download counts, package security/audit status.
- **Current-year (2026) WAA follow-up numbers:** WAA repo last pushed 2026-04-13 but no newer leaderboard retrieved; Windows-specific 2026 SOTA numbers unverified.

## Sources

1. **OmniParser — Screen Parsing tool for Pure Vision Based GUI Agent** — https://github.com/microsoft/OmniParser — fetched 2026-10-03 — **high** (primary repo README; 25,484★, pushed 2026-07-20 via `gh`).
2. **UFO³ / UFO² (microsoft/UFO) README** — https://github.com/microsoft/UFO — fetched 2026-10-03 — **high** (primary repo; 9,896★, pushed 2026-09-29).
3. **Windows Agent Arena: Evaluating Multi-Modal OS Agents at Scale — paper + repo** — https://arxiv.org/abs/2409.08264 (abstract + full HTML §4.2/Table 4) and https://github.com/microsoft/WindowsAgentArena — fetched 2026-10-03 — **high** (primary).
4. **Set-of-Mark Prompting Unleashes Extraordinary Visual Grounding in GPT-4V** — https://arxiv.org/abs/2310.11441 — Oct 2023 — fetched 2026-10-03 — **high** (primary abstract; code `microsoft/SoM`, 1,567★, dormant since 2024-08).
5. **UI-TARS: Pioneering Automated GUI Interaction with Native Agents** — https://arxiv.org/abs/2501.12326 — Jan 2025 — fetched 2026-10-03 — **high** (primary abstract; numbers author-claimed).
6. **OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks… + repo (OSWorld-Verified notice)** — https://arxiv.org/abs/2404.07972 (Apr 2024) and https://github.com/xlang-ai/OSWorld — fetched 2026-10-03 — **high** (3,171★, pushed 2026-09-14).
7. **WindowsAgentArena README (Navi modes, a11y backends, cost FAQ)** — https://github.com/microsoft/WindowsAgentArena — fetched 2026-10-03 — **high** (904★, pushed 2026-04-13).
8. **Playwright MCP README** — https://github.com/microsoft/playwright-mcp — fetched 2026-10-03 — **high** (37,771★, pushed 2026-09-28; npm `@playwright/mcp` 0.0.83 @ 2026-09-28).
9. **Cua README + cua-driver README (trycua/cua)** — https://github.com/trycua/cua and https://github.com/trycua/cua/blob/main/libs/cua-driver/README.md — fetched 2026-10-03 — **high** (27,847★, pushed 2026-10-03; Windows install script + Windows CI e2e referenced).
10. **Anthropic Computer Use Demo README** — https://github.com/anthropics/anthropic-quickstarts/tree/main/computer-use-demo — fetched 2026-10-03 — **high** (primary; repo served as `anthropics/claude-quickstarts`, 17,782★, pushed 2026-09-30).
11. **OS-Copilot README** — https://github.com/OS-Copilot/OS-Copilot — fetched 2026-10-03 — **medium** (1,794★ but **last push 2024-09-09** — dormant).
12. **Agent S (arXiv 2410.08164) + Agent S2 (arXiv 2504.00906)** — https://arxiv.org/abs/2410.08164 and https://arxiv.org/abs/2504.00906 — fetched 2026-10-03 (arXiv API); repo https://github.com/simular-ai/Agent-S (12,504★, pushed 2026-09-05) — **high**.
13. **ShowUI: One Vision-Language-Action Model for GUI Visual Agent** — https://arxiv.org/abs/2411.17465 — Nov 2024 — fetched 2026-10-03 — **high** (repo `showlab/ShowUI`, 1,910★).
14. **Aria-UI: Visual Grounding for GUI Instructions (ACL 2025)** — https://arxiv.org/abs/2412.16256 — fetched 2026-10-03 — **high**.
15. **SeeAction: Reverse Engineering How-What-Where of HCI Actions from Screencasts (ICSE 2025)** — https://arxiv.org/abs/2503.12873 — fetched 2026-10-03 — **high**.
16. **ShowUI-Aloha (arXiv 2601.07181, Jan 2026) + ShowUI-π (arXiv 2512.24965, Dec 2025)** — https://arxiv.org/abs/2601.07181 and https://arxiv.org/abs/2512.24965 — fetched 2026-10-03 — **medium** (unreviewed preprints).
17. **desktop-touch-mcp README (Windows 10/11 UIA MCP server)** — https://github.com/Harusame64/desktop-touch-mcp — fetched 2026-10-03 — **medium** (primary but self-reported; 21★, npm `@harusame64/desktop-touch-mcp` 2.0.0 @ 2026-09-24).
18. **Search + registry channels via CLI, 2026-10-03** — `gh search repos` (computer-use/VNC/RDP MCP servers; negative results for `windows-agent-adapter` and `FLADE`), 2 web searches (also negative), npm and PyPI queries (`@playwright/mcp`, `@harusame64/desktop-touch-mcp`, `@trycua/cua`, `cua` 0.2.0, `desktop-env` 1.0.2) — **medium** for repo-search claims (descriptions only), **high** for package versions.
19. **Introducing OSWorld-Verified (XLANG Lab blog)** — https://xlang.ai/blog/osworld-verified — 2025-07-28 — fetched 2026-10-03 — **high** (post by OSWorld's own authors; re-run numbers).
20. **UI-Evol (arXiv 2505.21964) + Minitap/AndroidWorld-100% (arXiv 2602.07787)** — https://arxiv.org/abs/2505.21964 and https://arxiv.org/abs/2602.07787 — fetched 2026-10-03 — **medium** (workshop paper + preprint; AndroidWorld baseline also from arXiv 2405.14573 fetched same run).



