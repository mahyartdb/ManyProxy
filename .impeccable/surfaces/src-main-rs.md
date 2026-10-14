---
version: 1
slug: "src-main-rs"
primary_target: "src/main.rs"
related_targets: []
---

# Surface brief: library list (inline loopback editing)

Scope: library row cards in `src/main.rs` (`LibraryDelegate::render_item`).
Visitor mode: Operate.

Audience, job: solo Windows operator scanning a personal proxy pool; per-row
they must see, change, and serve the loopback endpoint (`bind` + `port`) without
opening a dialog. Upstream stays dialog-owned; secrets never render inline.

Action/task: edit serve bind/port inline (Enter commits, Esc reverts), Start/Stop
and Edit that row in place. Proof/content: live collision + LAN-guard warnings
under the fields; honest toasts name rows; running rows warn restart-to-apply.

Constraints: pinned ids/strings stay (`row-dot-`, `row-name-`, `row-meta-`,
`row-port-`, `port-budget`, honest-toast copy, no-mutation guarantees); tests stay
green; 880px-minimum containment holds; dark + light ship equally.

Chosen direction: A-light strip-chart bench look (round2 `07`, round3 `r3-01…r3-05`
code-led schematics, no image comps). Single-line row cards: lamp + checkbox +
name/meta, fixed loopback bind + port fields, per-row Start/Stop + Edit, check
numeral preserved. Signature moment: Enter pulses the edit live with inline
validation; Esc snaps back to stored truth.

Unresolved: none blocking. Follow-ups (tray pilot lamp, feed-reel subscriptions)
are out of scope for this slice.

## Direction contract

THESIS: the loopback endpoint is the row — bind and port edit where they live,
and the row serves itself. This surface refuses the list-that-opens-forms
default: no dialog stands between seeing `127.0.0.1:10802` and changing it.

OWN-WORLD: A-light bench paper — quiet-surface cards on paper ground, hairline
rims, system-face bind/port fields at small size, ink primary Start, blue focus rings and
selection grammar intact, green serving / red failed / gray stopped unchanged.

STORY: the operator scans one-line rows, spots a stale port, types the new one,
sees the collision badge before committing, presses Enter, and gets a toast that
names the row. Esc always returns to stored truth; running edits warn restart.

FIRST VIEWPORT: title + five-menu bar with trailing master throw; search-with-count
and facet pickers plus bulk bar; row cards each carrying lamp, checkbox, name,
meta, fixed bind field, fixed port field, Start/Stop, Edit; inspector with tabs,
endpoints above the operate row; three-region status strip. All inside 880×620.

FORM: assigned grounded #7 strip-chart bench (seed f3baa0c8), translated to the
locked A-light look per user review; challenger raises carried as density courage
(fixed numeral columns), press discipline, and fixed trailing action cells.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
