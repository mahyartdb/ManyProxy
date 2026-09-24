---
name: cross-check
description: "Dual-independent adversarial verification with fix-until-convergent loop. Use when the user says 'cross-check', 'two reviewers', 'red-team this', or is shipping customer-facing/production output with no human review. Do NOT use for drafts, deterministic checks (use tests/lint), or single-diff correctness (use code-review)."
---

# Cross-Check

Multi-agent adversarial verification framework. Make a list, check it twice. If it's naughty, fix it until it's nice.

The core insight: a single agent reviewing its own output shares the same biases, knowledge gaps, and systematic errors that produced the output. Two independent reviewers with no shared context break this failure mode.

## When to Activate

Invoke this skill when:

- Output will be published, deployed, or consumed by end users
- Compliance, regulatory, or brand constraints must be enforced
- Code ships to production without human review
- Content accuracy matters (technical docs, educational material, customer-facing copy)
- Batch generation at scale where spot-checking misses systemic patterns
- Hallucination risk is elevated (claims, statistics, API references, legal language)

Do NOT use for internal drafts, exploratory research, or tasks with deterministic verification (use build/test/lint pipelines for those).

## Architecture

```
┌─────────────┐
│  GENERATOR   │  Phase 1: Make a List
│  (Agent A)   │  Produce the deliverable
└──────┬───────┘
       │ output
       ▼
┌──────────────────────────────┐
│     DUAL INDEPENDENT REVIEW   │  Phase 2: Check It Twice
│                                │
│  ┌───────────┐ ┌───────────┐  │  Two agents, same rubric,
│  │ Reviewer B │ │ Reviewer C │  │  no shared context
│  └─────┬─────┘ └─────┬─────┘  │
│        │              │        │
└────────┼──────────────┼────────┘
         │              │
         ▼              ▼
┌──────────────────────────────┐
│        VERDICT GATE           │  Phase 3: Naughty or Nice
│                                │
│  B passes AND C passes → NICE  │  Both must pass.
│  Otherwise → NAUGHTY           │  No exceptions.
└──────┬──────────────┬─────────┘
       │              │
    NICE           NAUGHTY
       │              │
       ▼              ▼
   [ SHIP ]    ┌─────────────┐
               │  FIX CYCLE   │  Phase 4: Fix Until Nice
               │              │
               │ iteration++  │  Collect all flags.
               │ if i > MAX:  │  Fix all issues.
               │   escalate   │  Re-run both reviewers.
               │ else:        │  Loop until convergence.
               │   goto Ph.2  │
               └──────────────┘
```

## Phase Details

### Phase 1: Make a List (Generate)

Execute the primary task. No changes to your normal generation workflow. Cross-check is a post-generation verification layer, not a generation strategy.

```python
# The generator runs as normal
output = generate(task_spec)
```

### Phase 2: Check It Twice (Independent Dual Review)

Spawn two review agents in parallel. Reviewers are STRICTLY READ-ONLY: do not modify any files. Output structured findings. A remediation plan may be offered, but the user must explicitly approve before any follow-up editing. Critical invariants:

1. **Context isolation** — neither reviewer sees the other's assessment
2. **Identical rubric** — both receive the same evaluation criteria
3. **Same inputs** — both receive the original spec AND the generated output
4. **Structured output** — each returns a typed verdict, not prose

### Extract Before Review

A reviewer needs the **artifact** and the **contract**, not the journey.

- Code: the diff or the function — not the whole file
- Decision: the proposal in 3–5 sentences plus the constraints it has to satisfy
- Assertion: the claim plus the evidence that supposedly supports it

Strip your reasoning. If you hand over conclusions, you'll get back validation of your conclusions. Pass ARTIFACT + CONTRACT only — never your conclusion about whether it passes. The unit must be small enough to hold in mind in one read; a 500-line PR gets decomposed first.

```python
REVIEWER_PROMPT = """
You are an independent quality reviewer. You have NOT seen any other review of this output.

## Task Specification
{task_spec}

## Output Under Review
{output}

## Evaluation Rubric
{rubric}

## Instructions
Evaluate the output against EACH rubric criterion. For each:
- PASS: criterion fully met, no issues
- FAIL: specific issue found (cite the exact problem)

Return your assessment as structured JSON:
{
  "verdict": "PASS" | "FAIL",
  "checks": [
    {"criterion": "...", "result": "PASS|FAIL", "detail": "..."}
  ],
  "critical_issues": ["..."],   // blockers that must be fixed
  "suggestions": ["..."]         // non-blocking improvements
}

Be rigorous. Your job is to find problems, not to approve. Do NOT validate. Do NOT summarize. Find issues, or state
explicitly that you cannot find any after thorough examination.
"""

**Framing decides the answer.** Prompt reviewers with "find what is wrong", never "is this good". The prompt above takes precedence over any reviewer persona's default balanced-verdict shape — paste it verbatim so it overrides.

#### Red-Team Method

Run each review this way:

1. **Extract every claim.** Separate **load-bearing** claims (if false, the output fails) from cosmetic ones. Only load-bearing claims are worth attacking.
2. **Steelman, then attack.** State the strongest version of why each load-bearing claim might be true — then attack _that_, not a strawman.
3. **Write each failure mode as "Fails if ___."** Concrete and falsifiable.
4. **Rank by (impact if wrong) × (likelihood wrong) × (cheapness to test).**
5. **Self-refute, don't fabricate.** Default to "this risk is real" unless the artifact cites evidence against it. Never invent a weakness the artifact doesn't have.
6. **For each surviving kill-assumption, give:** Fails if, evidence to get, kill criterion (threshold to stop or change course), cheapest test.
```

```python
# Spawn reviewers in parallel (subagents)
review_b = Agent(prompt=REVIEWER_PROMPT.format(...), description="Cross-Check Reviewer B")
review_c = Agent(prompt=REVIEWER_PROMPT.format(...), description="Cross-Check Reviewer C")

# Both run concurrently — neither sees the other
```

### Rubric Design

The rubric is the most important input. Vague rubrics produce vague reviews. Every criterion must have an objective pass/fail condition.

| Criterion             | Pass Condition                                                    | Failure Signal                                                    |
| --------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------- |
| Factual accuracy      | All claims verifiable against source material or common knowledge | Invented statistics, wrong version numbers, nonexistent APIs      |
| Hallucination-free    | No fabricated entities, quotes, URLs, or references               | Links to pages that don't exist, attributed quotes with no source |
| Completeness          | Every requirement in the spec is addressed                        | Missing sections, skipped edge cases, incomplete coverage         |
| Compliance            | Passes all project-specific constraints                           | Banned terms used, tone violations, regulatory non-compliance     |
| Internal consistency  | No contradictions within the output                               | Section A says X, section B says not-X                            |
| Technical correctness | Code compiles/runs, algorithms are sound                          | Syntax errors, logic bugs, wrong complexity claims                |

#### Domain-Specific Rubric Extensions

**Content/Marketing:**

- Brand voice adherence
- SEO requirements met (keyword density, meta tags, structure)
- No competitor trademark misuse
- CTA present and correctly linked

**Code:**

- Type safety (no `any` leaks, proper null handling)
- Error handling coverage
- Security (no secrets in code, input validation, injection prevention)
- Test coverage for new paths

**Compliance-Sensitive (regulated, legal, financial):**

- No outcome guarantees or unsubstantiated claims
- Required disclaimers present
- Approved terminology only
- Jurisdiction-appropriate language

### Phase 3: Naughty or Nice (Verdict Gate)

```python
def cross_check_verdict(review_b, review_c):
    """Both reviewers must pass. No partial credit."""
    if review_b.verdict == "PASS" and review_c.verdict == "PASS":
        return "NICE"  # Ship it

    # Merge flags from both reviewers, deduplicate
    all_issues = dedupe(review_b.critical_issues + review_c.critical_issues)
    all_suggestions = dedupe(review_b.suggestions + review_c.suggestions)

    return "NAUGHTY", all_issues, all_suggestions
```

Why both must pass: if only one reviewer catches an issue, that issue is real. The other reviewer's blind spot is exactly the failure mode cross-check exists to eliminate.

### Calibrated Verdict (Jev)

Reviewer PASS/FAIL is data; the NICE/NAUGHTY call above is orchestration, not measurement. When the `jev` skill is available (Jev MCP with `jev_check`, or `jev-code` CLI fallback + `TYPESAFE_API_KEY`), confirm borderline verdicts with a calibrated check instead of self-reported confidence:

- State: the artifact excerpt plus the merged issue list.
- Ask ONE `jev_check` call batching all disputed findings as parallel checks over that state: "Does this artifact clear every critical issue?" (ship only on yes at 0.75 or above) and, per disputed finding, "Is this flagged issue a false positive?" Mid-band 0.25-0.75 on any check escalates to the user with probabilities logged — never auto-ships.
- Record each probability beside the verdict in the result log.

Without Jev, the gate above stands as-is; mark the verdict `uncalibrated` in the log. A missing key never blocks the loop.

### Reconcile — Fold Findings Back

Reviewer output is data, not verdict. **You are still the orchestrator.** Re-read the artifact text against each finding before classifying — rubber-stamping the reviewer is the same failure mode as ignoring it. First matching class wins:

1. **Contract misread** — flagged only because the rubric/contract was unclear or incomplete. Fix the contract first.
2. **Valid + actionable** — real issue requiring a change. Change it, re-loop.
3. **Valid trade-off** — real but cost of fixing exceeds cost of accepting. Document it explicitly.
4. **Noise** — correct under context the reviewer didn't have. Note it, move on; ask whether richer contract context would have prevented the flag.

A fresh reviewer can be wrong because it lacks context. Don't defer just because it's "fresh."

**Doubt theater (checkable signal):** across 2+ rounds with substantive findings, zero classified actionable — you are validating, not verifying. Stop and escalate.

### Phase 4: Fix Until Nice (Convergence Loop)

```python
MAX_ITERATIONS = 3

for iteration in range(MAX_ITERATIONS):
    verdict, issues, suggestions = cross_check_verdict(review_b, review_c)

    if verdict == "NICE":
        log_cross_check_result(output, iteration, "passed")
        return ship(output)

    # Fix all critical issues (suggestions are optional)
    output = fix_agent.execute(
        output=output,
        issues=issues,
        instruction="Fix ONLY the flagged issues. Do not refactor or add unrequested changes."
    )

    # Re-run BOTH reviewers on fixed output (fresh agents, no memory of previous round)
    review_b = Agent(prompt=REVIEWER_PROMPT.format(output=output, ...))
    review_c = Agent(prompt=REVIEWER_PROMPT.format(output=output, ...))

# Exhausted iterations — escalate
log_cross_check_result(output, MAX_ITERATIONS, "escalated")
escalate_to_human(output, issues)
```

Critical: each review round uses **fresh agents**. Reviewers must not carry memory from previous rounds, as prior context creates anchoring bias. Stop early when a round returns only trivial or already-considered findings. If 3 cycles still surface substantive issues, the artifact may not be ready — surface that, don't grind a fourth alone. If 3 cycles is "obviously insufficient" because the artifact is large, the artifact is too big: return to Extract and decompose. Do not lift the bound.

## Synthesis Guardrails

You are both participant and synthesizer when you fix and re-verify:

- do not dismiss a reviewer flag without explaining why
- if a flag changed the output, say so explicitly
- always surface the strongest dissent, even if rejected
- never reconcile away real disagreement to tie a bow — legible dissent is the value

## Implementation Patterns

### Pattern A: Subagents (Recommended)

Subagents provide true context isolation. Each reviewer is a separate process with no shared state.

```bash
# In a session with a subagent tool, use it to spawn reviewers
# Both agents run in parallel for speed
```

```python
# Pseudocode for subagent invocation
reviewer_b = Agent(
    description="Cross-Check Review B",
    prompt=f"Review this output for quality...\n\nRUBRIC:\n{rubric}\n\nOUTPUT:\n{output}"
)
reviewer_c = Agent(
    description="Cross-Check Review C",
    prompt=f"Review this output for quality...\n\nRUBRIC:\n{rubric}\n\nOUTPUT:\n{output}"
)
```

### Pattern B: Sequential Inline (Fallback)

When subagents aren't available, simulate isolation with explicit context resets:

1. Generate output
2. New context: "You are Reviewer 1. Evaluate ONLY against this rubric. Find problems."
3. Record findings verbatim
4. Clear context completely
5. New context: "You are Reviewer 2. Evaluate ONLY against this rubric. Find problems."
6. Compare both reviews, fix, repeat

The subagent pattern is strictly superior — inline simulation risks context bleed between reviewers.

### Pattern C: Batch Sampling

For large batches (100+ items), full cross-check on every item is cost-prohibitive. Use stratified sampling:

1. Run cross-check on a random sample (10-15% of batch, minimum 5 items)
2. Categorize failures by type (hallucination, compliance, completeness, etc.)
3. If systematic patterns emerge, apply targeted fixes to the entire batch
4. Re-sample and re-verify the fixed batch
5. Continue until a clean sample passes

```python
import random

def cross_check_batch(items, rubric, sample_rate=0.15):
    sample = random.sample(items, max(5, int(len(items) * sample_rate)))

    for item in sample:
        result = cross_check_full(item, rubric)
        if result.verdict == "NAUGHTY":
            pattern = classify_failure(result.issues)
            items = batch_fix(items, pattern)  # Fix all items matching pattern
            return cross_check_batch(items, rubric)   # Re-sample

    return items  # Clean sample → ship batch
```

## Failure Modes and Mitigations

| Failure Mode            | Symptom                                       | Mitigation                                                                                                |
| ----------------------- | --------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Infinite loop           | Reviewers keep finding new issues after fixes | Max iteration cap (3). Escalate.                                                                          |
| Rubber stamping         | Both reviewers pass everything                | Adversarial prompt: "Your job is to find problems, not approve."                                          |
| Subjective drift        | Reviewers flag style preferences, not errors  | Tight rubric with objective pass/fail criteria only                                                       |
| Fix regression          | Fixing issue A introduces issue B             | Fresh reviewers each round catch regressions                                                              |
| Reviewer agreement bias | Both reviewers miss the same thing            | Mitigated by independence, not eliminated. For critical output, add a third reviewer or human spot-check. |
| Cost explosion          | Too many iterations on large outputs          | Batch sampling pattern. Budget caps per verification cycle.                                               |

## Independent Lenses

Review through independent lenses in parallel subagents. Each lens sees the content, never another lens's findings. The adversarial lens requires at least ten concrete findings and treats an empty list as a signal to re-check.

Each finding carries: lens, location (file:line-range for code, section for documents), trigger_condition (the problem, or the condition that exposes it, in one line), guard_snippet (the concrete fix, guard, or missing check), potential_consequence (what goes wrong if it ships as-is).

Optional (technical-writing): for docs or copy artifacts, run technical-writing + unslop lenses before the verdict.

## Cost Analysis

Cross-check costs approximately 2-3x the token cost of generation alone per verification cycle. For most high-stakes output, this is a bargain:

```
Cost of cross-check = (generation tokens) + 2×(review tokens per round) × (avg rounds)
Cost of NOT cross-check = (reputation damage) + (correction effort) + (trust erosion)
```

For batch operations, the sampling pattern reduces cost to ~15-20% of full verification while catching >90% of systematic issues.

## Next

- NICE → `/review` (or `/finish` when no review gate remains).
- NAUGHTY fixes → `/implement`, ticket-scoped — never direct-commit. Then `/converge` → `/review`. MAX 3 rounds total including the outer `implement`/`converge` budget, then escalate.
- Unknown cause behind a flag → `/debug` before fixing.

## Optional

- test-driven-development (RED-GREEN-refactor for the fix)
- security-bounty-hunter (reachable-exploit triage on security-flagged output)
