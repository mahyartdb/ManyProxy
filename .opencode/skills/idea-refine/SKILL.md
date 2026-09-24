---
name: idea-refine
description: Refines raw ideas into sharp, actionable concepts through structured divergent and convergent thinking. Use when an idea is still vague, when you need to stress-test assumptions before committing to a plan, or when you want to expand options before converging on one. Triggers on "ideate", "refine this idea", or "stress-test my plan".
---

# Idea Refine

Refines raw ideas into sharp, actionable concepts worth building through structured divergent and convergent thinking.

Pressure-test the idea in conversation, while changing your mind is still cheap, until it becomes something the user can act on with conviction or reject. The main goal is better thinking, not producing an artifact. Do not steer the conversation toward "shall we build it?" Lead by questioning, not lecturing. Ask one question at a time, press on weak points, and do not let vague claims pass without examination.

**Asking mechanics belong to `ask-me`.** All questioning in this skill runs through it: design-tree rounds, the frontier, a GUESS per question, the explicit-yes gate. idea-refine owns the thinking shapes below (triage, lenses, risk gate, forcing questions, scorecard); `ask-me` owns how questions are asked and when understanding is shared.

## Triage

Classify the request out loud before the first question, so the partner can override:

- **Spike** — a feasibility question ("can we...", "quick and dirty is fine") whose output is an answer, not code you keep. Present the question and what you'll try in 2-3 sentences, get a nod, then find out as cheaply as correctness allows. Report findings as a recommendation; anything built stays labeled throwaway.
- **Bounded** — a well-scoped change to a flow that already exists here to read. Present a short design in chat and STOP. Implementation starts only after yes to that design.
- **Architectural** — new projects, new subsystems, changes that restructure how components fit together. Follow the full process: questions, approaches, design, spec, plan.

When in doubt between two paths, take the heavier one. Hidden complexity discovered mid-task upgrades the path one way only — never downgrades mid-task.

<HARD-GATE>
Before any implementation action, complete the path prerequisites. Approval of an idea does not approve artifacts that don't exist yet. Resume at the earliest incomplete stage.
</HARD-GATE>

## Entry

Get from the user:

- The idea in 1–2 sentences (the _what_)
- The reason it's on their mind (the _why now_)
- Any starting context (a chat where this came up, a tweet that inspired it, a problem they've hit)

If the idea is too vague to score, ask 1–2 clarifying questions and stop. Don't pad the brief with assumptions.

Capture without judging: quote the original wording (redact tokens, passwords, keys, cookies; sanitize credential-bearing URLs), restate it in one or two neutral sentences, record origin and trigger (who raised it, what prompted it — mark unknowns `[NEEDS CLARIFICATION]`), note the idea type (`new-capability | improvement | fix | exploration | cost-saving | compliance | other`), and list first-glance unknowns without answering them. No feasibility verdicts, no design — that belongs downstream.

## How It Works

1.  **Understand & Expand (Divergent):** Restate the idea as a crisp "How Might We" problem statement. Ask 3–5 sharpening questions — no more. Do NOT proceed until you understand who this is for and what success looks like. Generate 5–8 idea variations using these lenses: `Inversion:` "What if we did the opposite?" `Constraint removal:` `Audience shift:` `Combination:` `Simplification:` "What's the version that's 10x simpler?" `10x version:` `Expert lens:`. Start with the user experience, work backwards to technology. Don't generate 20+ ideas. Don't over-engineer the process.
2.  **Evaluate & Converge:** Cluster ideas, stress-test them, and surface hidden assumptions. Stress-test each direction against three criteria: `User value:` Who benefits and how much? Is this a painkiller or a vitamin? `Feasibility:` `Differentiation:`. Say no to 1,000 things. Focus beats breadth. Be honest, not supportive. Don't be a yes-machine. Don't just list ideas — tell a story.
3.  **Sharpen & Ship:** Produce a concrete markdown one-pager moving work forward: problem, options considered, chosen direction, open questions. What you're choosing to ignore (and why that's okay for now). Key Assumptions to Validate, each with how to test it. The "Not Doing" list is arguably the most valuable part. Ask the user if they'd like to save this; only save if they confirm.

## Working Backwards (optional)

Write the press release announcing the finished product before building it. If you can't write a compelling press release, the product isn't ready. The customer FAQ validates the value proposition from the outside in. The internal FAQ addresses feasibility, risks, and hard trade-offs.

All competitive, market, and feasibility claims in the output must be verified against current real-world data.

## Pre-Build Risk Gate

Review the idea across these risks:

- **Demand:** Is there evidence that a specific buyer or user urgently wants this?
- **Positioning:** Can the target user understand what it is and why it matters in one sentence?
- **Monetization:** Is there a credible path to payment, budget, or strategic value?
- **Retention:** Is there a reason users would return after the first try?
- **Trust:** Does the product require credibility, data access, integrations, or behavior change that users may resist?
- **Distribution:** Is there a repeatable way to reach the target user?
- **Feature adoption:** For feature work, will the feature change user behavior or just add surface area?

Keep the response short and decision-oriented:

1. **Risk verdict:** Low, medium, or high risk, with one sentence explaining why.
2. **Main assumption:** The single assumption most likely to break the project.
3. **Evidence to find first:** The smallest useful signal before building more.
4. **Do next:** One concrete validation step or reduced build scope.
5. **Delay:** What not to build yet.

## The six forcing questions

Ask ONE AT A TIME. Push on each one until the answer is specific, evidence-based, and uncomfortable.

#### Q1: Demand Reality

**Ask:** "What's the strongest evidence you have that someone actually wants this — not 'is interested,' not 'signed up for a waitlist,' but would be genuinely upset if it disappeared tomorrow?"

**Push until you hear:** Specific behavior. Someone paying. Someone expanding usage. Someone building their workflow around it. Someone who would have to scramble if you vanished.

**Red flags:** "People say it's interesting." "We got 500 waitlist signups." "VCs are excited about the space." None of these are demand.

#### Q2: Status Quo

**Ask:** "What are your users doing right now to solve this problem — even badly? What does that workaround cost them?"

**Push until you hear:** A specific workflow. Hours spent. Dollars wasted. Tools duct-taped together. People hired to do it manually.

**Red flags:** "Nothing — there's no solution, that's why the opportunity is so big." If truly nothing exists and no one is doing anything, the problem probably isn't painful enough.

#### Q3: Desperate Specificity

**Ask:** "Name the actual human who needs this most. What's their title? What gets them promoted? What gets them fired? What keeps them up at night?"

**Push until you hear:** A name. A role. A specific consequence they face if the problem isn't solved.

**Red flags:** Category-level answers. "Healthcare enterprises." "SMBs." "Marketing teams." These are filters, not people. You can't email a category.

#### Q4: Narrowest Wedge

**Ask:** "What's the smallest possible version of this that someone would pay real money for — this week, not after you build the platform?"

**Push until you hear:** One feature. One workflow. Something shippable in days that someone would pay for.

**Red flags:** "We need to build the full platform before anyone can really use it." Attached to architecture, not value.

**Bonus push:** "What if the user didn't have to do anything at all to get value? No login, no integration, no setup. What would that look like?"

#### Q5: Observation & Surprise

**Ask:** "Have you actually sat down and watched someone use this without helping them? What did they do that surprised you?"

**Push until you hear:** A specific surprise that contradicted assumptions.

**Red flags:** "We sent out a survey." "We did some demo calls." Surveys lie. Demos are theater.

#### Q6: Future-Fit

**Ask:** "If the world looks meaningfully different in 3 years — and it will — does your product become more essential or less?"

**Push until you hear:** A specific claim about how their users' world changes and why that change makes the product more valuable.

**Red flags:** "The market is growing 20% per year." Growth rate is not a vision.

## Response posture

Take a position on every answer. State your position AND what evidence would change it. Push once, then push again. The first answer is the polished version; the real one comes after the second or third push. Calibrated acknowledgment, not praise. Agreement is allowed only when it helps the user think better. Praise is noise. Start by scrutinizing the idea, not endorsing it. Never say "That's an interesting approach" — take a position instead. Never say "There are many ways to think about this" — pick one and state what evidence would change your mind. If they're wrong, say they're wrong and why.

## Problem Definition

Frame the problem before shaping options — problem space only, never solution. If the input arrived as a solution ("build X"), reverse-engineer the underlying problem it solves.

- **Statement**: one or two sentences — who is affected, what hurts, under what conditions, why now. No features, no architecture.
- **Users & stakeholders**: who experiences it vs who decides/funds/is impacted. Cite evidence; mark inventions `[NEEDS CLARIFICATION]`.
- **Goals**: outcomes that make solving it worthwhile. **Non-goals**: explicitly out of scope, to bound work and prevent creep.
- **Success metrics**: how you'd know it worked — measurable signals preferred, qualitative labeled as such.
- **Cost of inaction**: what happens if nothing is built. This is what `decide` weighs against.
- **Open questions**: carried forward, never silently dropped.

Never invent users, metrics, or goals unsupported by intake/research. If the problem cannot be articulated at all, say so and route back to Entry/research rather than forcing a statement.

## Scorecard

For each surviving direction, give a 1-line take + a verdict (strong / OK / weak / unknown — needs research): Problem, Audience, Wedge, Monetization, Moat, Portfolio fit, Distribution, Energy fit, Opportunity cost. Don't BS the unknowns. Mark them and route to research. Verdict per idea: build / sleep on it / pass.

Calibrated scoring (Jev): when the `jev` skill is available, score all directions in one `jev_score` call (items: one-pager + evidence per direction; levels strong / OK / weak / unknown with the rubric above as criteria) and record scores beside the verdict. Unknowns stay unknowns — a low score is not evidence. `review` decisions escalate to the human; `auto` sticks. Without Jev, score by judgment and mark `uncalibrated`.

Shape options at concept level, not spec: 2–3 distinct options spanning the trade-off space (always include the smallest thing that could work, and where relevant do-nothing/buy-instead). Per option: one-paragraph sketch of what the user experiences, **appetite as budget** (`small` = days, `medium` = weeks, `large` = months — never an estimate), trade-offs (wins vs sacrifices), and **rabbit holes** (parts most likely to blow up scope). Recommend one option tied to goals and metrics — or explicitly recommend not proceeding. Restate out-of-scope for the pick and list the assumptions it depends on. Recommending none is valid; manufacturing a winner is not.

## Hard gate

Classify the work aloud (spike, bounded, architectural) so the partner can override. No implementation before approval. Write back understanding first: intended outcome, constraints, success criteria — separate what they said from assumptions, invite correction before treating it as the brief. Hidden complexity upgrades the path one way only, never downgrades mid-task.

## Anti-patterns

- "Too simple to need approval" — every path ends with approval; scale the artifact, don't skip gates.
- Premature convergence: picking the first plausible option.
- Solution-first: specifying implementation before the problem is sharp.

## Next

- Settled one-pager → `decide` (verdict before any delivery work).
- Unknowns marked `[NEEDS CLARIFICATION]` or `unknown` verdicts → `research` (findings come back here for re-shaping).
- Ambiguous tradeoff between surviving directions → `council` (optional; recommendation only, then back here or to `decide`).
- Feasibility question → `prototype` skill (decision only, then refine; the artifact never ships).
