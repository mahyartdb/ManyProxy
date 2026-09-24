---
name: ask-me
description: Structured interrogation to extract hidden requirements before planning. Use when the user says "grill me", "ask me questions", or "stress-test my thinking" on a half-formed idea. Do NOT use for scored triage (use idea-refine), go/kill verdicts (use decide; council recommends only), or settled specs (use spec/checklist). Asks; never builds.
---

# Ask Me

Interview the user relentlessly until you reach a shared understanding. Map this as a **design tree**: every decision branches into the decisions that hang off it.

This skill is the questioning engine inside `idea-refine`: idea-refine owns what to think (triage, lenses, risk gate, scorecard); ask-me owns how to ask (rounds, frontier, GUESS, explicit-yes gate). Invoke directly for any non-trivial ask outside idea-refine.

What people ask for and what they actually want are different things. The cheapest moment to find the gap is before any plan, spec, or code exists: once building starts, switching costs are real, and the user will rationalize the wrong thing into a "good enough" thing.

## Before the first round

State your hypothesis and confidence:

```
HYPOTHESIS: <what you think they actually want, and why>
CONFIDENCE: <~%, plus exactly what is missing>
```

Check the ask for who it's for, why they want it, what success looks like, and the binding constraint. Including "Out of scope" is non-negotiable. Half of misalignment is silent disagreement about what is _not_ being built.

## Rounds

Work the tree in **rounds**. The **frontier** is every decision whose prerequisites are already settled: the questions you can ask _now_ without guessing at answers you haven't heard yet. Ask the whole frontier in one round: number each question and give your recommended answer. Then wait for the user's answers before the next round.

Format a round like so:

```
❓ **Q1** - **<question title>**: <question body, might be multiple paragraphs, including multiple choices>

➡️ <your recommended answer>

---

❓ **Q2** - **<question title>**: <question body, might be multiple paragraphs, including multiple choices>

➡️ <your recommended answer>
```

Each round the user answers reshapes the tree: settled decisions push the frontier outward and unblock questions that depended on them. Recompute the frontier and ask the next round. A question whose answer depends on another question still open in this round belongs to a _later_ round, not this one. Ask 1-5 questions in the first pass. Prefer questions that eliminate whole branches of work. Only include questions whose answers materially impact architecture, data modeling, task decomposition, test design, UX behavior, operational readiness, or compliance validation. Optimize for scannability (short, numbered questions; avoid paragraphs). Offer multiple-choice options when possible. Include a fast-path response (e.g., reply `defaults` to accept all recommended/default choices).

Encode answers back into the calling artifact (one-pager / spec / plan).

## Question shape

One question at a time, each carrying your best guess:

```
GUESS: <your hypothesis for the answer, with the reasoning that produced it>
```

Wait for the user to react before asking the next question. Ask things like: _"If you didn't have to justify this to anyone, what would you actually want?"_

## Confirm — explicit yes, not hollow yes

The gate is an explicit "yes." The following are **not** yes:

- "Whatever you think is best." → The user is delegating, which means they don't have 95% confidence either. Re-ask with two concrete options framed as a choice.
- "Sounds good." → Ambiguous. Ask: "Anything you'd refine?" Silence isn't confirmation.
- "Sure, let's go." → Often a polite exit, not an endorsement. Same follow-up.
- Silence followed by "okay let's start." → The user has given up on the interview, not converged. Stop and ask whether you've missed something.

If they correct you, fold the correction in and restate. Loop until you get an explicit yes.

## When not to grill

- The ask is unambiguous and self-contained ("rename this variable", "fix this typo")
- The user has explicitly asked for speed over verification
- Pure information requests ("how does X work?", "what does this code do?")
- Mechanical operations (renames, formats, file moves)
- You already have ≥95% confidence; re-read the stop condition below before assuming you don't

## Facts vs decisions

Finding _facts_ is your job, never the user's. When a frontier question needs a fact from the environment (filesystem, tools, etc.), go find it; don't ask the user for anything you could look up yourself. Don't block on it: a running exploration is an unsettled prerequisite, so only the questions downstream of it wait; ask the rest of the frontier now. The _decisions_ are the user's: put each to them and wait.

## Stop condition

You're done when you can answer yes to this:

> _Can I predict the user's reaction to the next three questions I would ask?_

If yes, you have shared understanding. Stop interviewing and produce the restate. If no, you're not done; ask the next question. The session is done when the frontier is empty: every branch of the design tree visited, nothing left silently assumed. Do not act on it until the user confirms you have reached a shared understanding.

## Guard

This skill needs a live, responsive user. **Do not invoke in non-interactive contexts** like CI pipelines, scheduled runs, or autonomous loops. If you're in one of those and the ask is underspecified, flag that as a blocker for the user instead of guessing.

## Next

- Inside `idea-refine`, return to `idea-refine` with the answers (re-shape the one-pager there).
- Standalone, route by artifact: intent taking shape → `/refine`; settled intent → `/spec`; settled spec → `/plan`.
- Never route to `/implement` — questioning produces understanding, not build authorization.
- Non-interactive contexts including `qmode` autonomous → flag blocker per Guard above.
