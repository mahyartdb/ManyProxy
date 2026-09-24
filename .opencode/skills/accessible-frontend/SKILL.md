---
name: accessible-frontend
description: Accessibility depth for UI work: contrast, keyboard, focus, landmarks, forms, live regions, motion safety. Use when building or auditing UI for a11y, or when the user says "accessible", "screen reader", "keyboard nav", "WCAG". Do NOT use for visual taste (use impeccable/design-taste-frontend), token files (use design-md), or motion values (use animate/gsap-*). Gates stay with the impeccable audit and design-md lint.
---

# Accessible Frontend

Nobody else owns this: contrast math, keyboard operability, focus visibility, landmarks, form labeling, live regions, motion safety. Visual gates stay in the `impeccable` audit; token contrast stays in `design-md` lint. This skill is the depth behind both.

## Depth reference

Load `references/human-depth.md` when the task builds or edits UI. Same Tell/Why/Fix shape throughout: contrast (4.5:1 text, 3:1 large text and non-text), keyboard (reach + operate + Escape), focus (visible, 3:1, every theme), states (text-first, never color-only), landmarks/headings/labels, skip links, forms (for/id, aria-describedby, announced errors), live regions and dialogs (aria-live, focus trap + return, Escape), motion safety (reduced-motion, pause controls), zoom and mobile keyboard.

## Contrast checker

Never assert a pairing passes. Run `contrast-check.py` (needs python3; formula + reference table in the depth doc are the fallback):

```bash
python "qstack/skills/accessible-frontend/contrast-check.py" "#FFFFFF" "#777777"
# ratio: 4.48:1 — normal text (4.5:1): FAIL, large text (3.0:1): PASS
```

Exit code 0 only when both verdicts pass. `contrast-check.py --selftest` verifies the table against the formula (8 pairs must read OK).

## Done when

- [ ] Every checklist item in the depth reference answers yes
- [ ] No asserted pairings; every ratio computed, worst spots checked (text over images/gradients included)
- [ ] Keyboard-only pass completed; screen-reader announcements verified for dynamic content

## Related

- Skills: `web-design-guidelines` (visual/interaction rules behind the checklist)
