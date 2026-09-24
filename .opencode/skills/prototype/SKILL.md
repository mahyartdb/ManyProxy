---
name: prototype
description: Throwaway spike answering one design question — clickable state-model demo or divergent UI variations on a single route. Use when the user says "prototype", "spike this", or "try variations before we commit". Do NOT use for production builds (use implement/qmode), plan artifacts (use spec/to-tickets), or motion builds (use animate). Delete or absorb when done.
---

# Prototype

## Pick a branch

- "Does this logic / state model feel right?" → [LOGIC.md](LOGIC.md). A single, self-contained HTML file that lets anyone drive a state model by clicking buttons. If the question is "what should this look like," this is the wrong branch.
- "What should this look like?" → [UI.md](UI.md). Generate several radically different UI variations on a single route, switchable from a floating bottom bar. If the question is about logic/state rather than what something looks like, this is the wrong branch.

Getting this wrong wastes the whole prototype. If the question is genuinely ambiguous, state the assumption at the top of the prototype.

## Rules that apply to both

- Throwaway from day one, and clearly marked as such. A prototype that needs tests is no longer a prototype.
- One command to run. One file the recipient double-clicks; a React app or a dev server defeats "shareable".
- No persistence by default. State lives in memory.
- Skip the polish. No tests, no error handling beyond what makes the prototype runnable, no abstractions, no generalise. The point is to learn something fast and then delete it.
- Surface the state. After every action, print or render the full relevant state so the user can see what changed.
- Don't ship the prototype into production. Fold the validated decision into the real code — don't leave it rotting in the repo. The answer is the only thing worth keeping: capture it somewhere durable along with the question it was answering.

## Pick discipline

- Derive the rubric. State what success looks like for this task, then turn it into 3–6 concrete gradeable criteria.
- Each candidate writes to its own location. Never mix branches in one artifact.
- Record the pick and the reason in a short synthesis note alongside the base artifact, including the cross-judge's verdict.

## Divergence discipline

- Each variant states its name and axis (layout, density, personality, motion). If two drafts come out too similar, redo one. Three slightly-tweaked card grids isn't a UI prototype, it's wallpaper.
- One variant at a time, full-size, in realistic context. Never side-by-side thumbnails.

## Next

- Decision only, never artifact: carry the answer back to `/refine` | `/spec` | `/plan`; a decision-rich snippet (state machine, reducer, schema, type shape) may be carved out into `to-tickets` — then delete the prototype.
- Scope change surfaces a new question → back to `idea-refine` (re-shape before re-spiking).
