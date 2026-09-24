# Accessible Frontend Depth

> Accessibility depth for UI work: the parts of a UI that exclude people.

> Companion to the `accessible-frontend` skill. Load it when the task builds or edits UI: contrast, keyboard, focus, states, landmarks, forms, live regions, and the mobile details that exclude people.

## How to use this skill

- Load together with the `accessible-frontend` skill whenever the task builds or edits UI.
- Every entry has the same shape: **Tell** (the pattern), **Why** (who it excludes, and why it reads as unfinished), **Fix** (what to do instead), with the governing WCAG criterion cited.
- Accessibility is not a checklist of extras bolted on at the end. The checklist at the end of this file runs alongside the `impeccable` audit or `design-md` lint gate.
- The contrast checker (formula + reference table + script) lives in this skill. Use it for every color pairing you cannot verify by eye.
- This skill keeps only the mobile details that exclude people: zooming, and the on-screen keyboard.

## Color & Contrast

### Low-Contrast Text

- **Tell:** light grey text on a white or near-white background, thin body text, muted labels chosen because they look "elegant" but are hard to read.
- **Why:** it excludes low-vision users and everyone in bright light. It is a visual choice made without checking the standard.
- **Fix:** meet WCAG AA minimums (WCAG 1.4.3): 4.5:1 for normal text, 3:1 for large text (18px+). Compute the ratio; do not eyeball it.

### Text Over a Photo or Gradient

- **Tell:** white text placed directly over an image or gradient that is light in some areas, checked at one bright spot only.
- **Why:** contrast is local. Where the image is light, the text drops below 4.5:1 even if the hero still "looks" fine. WCAG 1.4.3 requires testing the whole area the text passes over, not a single point.
- **Fix:** add a scrim or a solid color block behind the text, then verify the worst spot, not the best. If any part of the text area fails, the treatment fails.

### The Grey-on-Grey Hallucination

- **Tell:** "dark grey on black" or "light grey on white" claimed to pass AA without a computation.
- **Why:** this is the most common accessibility hallucination. The eye overestimates contrast on grey pairs, and agents repeat the claim because it sounds plausible. #555555 on black is 2.8:1. It fails.
- **Fix:** never assert a pairing passes. Run the contrast checker (below) or apply the formula. When neither is possible, use the reference table.

### Non-Text Contrast

- **Tell:** interactive components (buttons, icons, input borders, focus indicators, chart segments) distinguished from their background by less than 3:1.
- **Why:** non-text UI carries information by shape and edge. When the edge is a hair of tint, low-vision users cannot find the control. The same bar applies to states like hover and selected.
- **Fix:** give every component boundary and every status indicator a 3:1 ratio against adjacent colors (WCAG 1.4.11). Pair icons with a text label that meets 4.5:1.

### The Contrast Checker

The home of the contrast checker. Three layers, from most to least convenient:

**The script.** When a script runtime is available and the file is present, run it instead of computing by hand. The script ships in this skill's folder (`contrast-check.py`, next to this file). Run it with `python3` on macOS and Linux, `python` on Windows:

```bash
python3 "qstack/skills/accessible-frontend/contrast-check.py" "#FFFFFF" "#777777"
# normal text: FAIL (4.48 < 4.5)
# large text:  PASS (4.48 >= 3.0)
```

If the `${CLAUDE_SKILL_DIR}` variable is not available in this agent, point the script path at this skill's folder directly. The script exists so agents stop hallucinating AA. It takes two hex colors and prints the ratio and the verdict for both text sizes. If the file is missing, the formula and table below are complete on their own. Never block on the script.

**The formula (WCAG 2.x).**

1. Contrast ratio = (L1 + 0.05) / (L2 + 0.05), where L1 is the lighter relative luminance and L2 the darker.
2. Relative luminance L of one color: convert each channel to 0-1 (`c = hex / 255`), then linearize: if `c <= 0.03928`, `c_lin = c / 12.92`; otherwise `c_lin = ((c + 0.055) / 1.055)^2.4`.
3. `L = 0.2126*R + 0.7152*G + 0.0722*B`.
4. Round the ratio to two decimals and compare: 4.5:1 for normal text, 3:1 for large text (18px+, per WCAG 1.4.3). The maximum ratio is 21.0 (black on white).

**The reference table** (common pairings, computed with the formula):

| Pairing (text on background) | Ratio | Normal text (4.5) | Large text (3.0) |
| ---------------------------- | ----- | ----------------- | ---------------- |
| Black on white               | 21.00 | Pass              | Pass             |
| White on black               | 21.00 | Pass              | Pass             |
| White on #333333             | 12.63 | Pass              | Pass             |
| White on #666666             | 5.74  | Pass              | Pass             |
| #777777 on white             | 4.48  | Fail              | Pass             |
| White on #888888             | 3.54  | Fail              | Pass             |
| White on #999999             | 2.85  | Fail              | Fail             |
| #555555 on black             | 2.82  | Fail              | Fail             |

Read the table as a sanity check, not as a substitute. Any pairing not listed, or anything near a threshold, goes through the formula or the script.

## Keyboard

### Removed Focus Outline

- **Tell:** `outline: none` or `outline: 0` with no replacement focus style.
- **Why:** keyboard users cannot see where they are. It is the fastest way to make a UI unusable without a mouse, and keyboard access forbids it outright.
- **Fix:** keep or replace the outline with a visible `:focus-visible` style that meets the same contrast bar (3:1 against its neighbors). Never set `outline: none` without a replacement.

### Mouse-Only Patterns

- **Tell:** menus that open on hover only, dropdowns that click-open but do not keyboard-open, drag-and-drop with no keyboard fallback.
- **Why:** each one excludes keyboard and assistive-technology users (WCAG 2.1.1). If a control cannot be reached and operated by Tab, Enter, or Space, it does not exist for a whole group of people.
- **Fix:** every interactive element is reachable and operable by keyboard (WCAG 2.4.3): logical tab order following visual order, activation with Enter or Space, and dialogs closable with Escape.

### Broken Tab Order

- **Tell:** focus jumps around the page, skips content, or lands on hidden elements because the DOM order does not match the visual order.
- **Why:** tab order that reads the code order instead of the visual order makes navigation unpredictable (WCAG 2.4.3). Users lose their place and the page feels broken.
- **Fix:** keep the source order matching the visual order, add skip links for long pages, and never give real content `tabindex="-1"` unless it is part of a controlled focus trap such as a dialog.

## Focus & States

### Weak or Invisible Focus Indicator

- **Tell:** a focus ring the same color as the background, a ring that only appears on hover, or an indicator thinner than a 1px border.
- **Why:** the focus indicator is how keyboard users know where they are. If it fails the contrast bar or only shows on hover, keyboard-only use breaks (WCAG 2.1.1, 1.4.11).
- **Fix:** a visible focus indicator on every interactive element, 3:1 against adjacent colors, in every theme you ship. Check it in dark and light mode.

### Color-Only Feedback

- **Tell:** success, error, and status communicated only by color: red error text, green success border, a tinted chip, with no icon, label, or text.
- **Why:** it excludes color-blind and low-vision users, and it disappears entirely in forced-colors mode. A status that depends on seeing hue is not a status.
- **Fix:** pair every color signal with text, an icon, or a pattern. Error states are text first: "Password must be at least 8 characters", not just a red border.

### Missing UI States

- **Tell:** a data view with no empty, loading, or error state, or states that exist but are invisible: a spinner with no text, an empty screen with no explanation.
- **Why:** Every data view requires the three states; the accessibility angle is that each must be perceivable and informative, not decorative. A loading spinner with no context reads as a frozen page to screen-reader users.
- **Fix:** every data view has all three states (empty/loading/error), each announced or visible: an explicit empty message, a loading state with text, and an error state that says what happened and how to proceed.

## Zoom & Mobile Use

The layout mechanics behind mobile (breakpoints, scale, grids, overflow, tap targets) are the concern of the `css` skill. This section keeps only the mobile details that exclude people: zooming, and the on-screen keyboard.

### Text That Cannot Zoom

- **Tell:** fixed pixel font sizes, or containers with `overflow: hidden` that clip text at 200% zoom.
- **Why:** users must be able to resize text (WCAG 1.4.4). If zooming to 200% clips the content or forces horizontal scroll, the text is not resizable in practice.
- **Fix:** fluid type that reflows with zoom, no clipping containers on text, and verify the layout holds at 200% zoom on a narrow viewport.

### Mobile Keyboard Covers the Form

- **Tell:** inputs at the bottom of the viewport hidden behind the on-screen keyboard, with no scroll-into-view and no room for the input.
- **Why:** a form the user cannot see or reach is a form they cannot complete. It is a mobile-only exclusion (WCAG 1.4.10 reflow).
- **Fix:** when an input is focused, it scrolls into view above the keyboard, with enough bottom padding that the focused field is never covered. Test with a real device or an emulated keyboard.

## Human Skill Checklist

Run these when the task involves UI, alongside the `impeccable` audit or `design-md` lint gate. All answers must be **yes**:

- [ ] Is every text and background pairing verified against the contrast checker (formula, table, or script), including text over images and gradients? (WCAG 1.4.3)
- [ ] Does every interactive component boundary and status indicator meet 3:1 against its background? (non-text contrast)
- [ ] Is the focus indicator visible, high-contrast, and present on every interactive element in every theme? (WCAG 2.1.1, 1.4.11)
- [ ] Is every interactive element reachable and operable by keyboard, with dialogs closable via Escape and no `outline: none` without a replacement? (WCAG 2.1.1)
- [ ] Are the empty, loading, and error states of every data view present and perceivable, not color-only?
- [ ] Can text be resized to 200% without being clipped, and does the mobile keyboard never cover a focused input?
- [ ] Do landmarks, headings, labels, and live regions hold (sections below)?

## Landmarks, Headings, Labels

- **Tell:** div soup with no header/main/nav/footer; skipped heading levels; images without alt; icon-only buttons with no accessible name.
- **Why:** screen-reader users navigate by landmarks and headings. An unnamed control is announced as "button" with no purpose; a missing alt leaves an image-shaped hole.
- **Fix:** use header/main/nav/footer/aside landmarks; exactly one h1, never skip levels; every img gets alt (alt="" only for decorative); every icon button gets an aria-label; never use a placeholder as the sole label.

## Skip Links

- **Tell:** long pages force keyboard users through the entire nav on every visit.
- **Why:** without a bypass, keyboard navigation cost scales with page size (WCAG 2.4.1).
- **Fix:** the first focusable element is a "Skip to content" link to #main, visible on focus.

## Forms

- **Tell:** labels as placeholders only; errors shown by red border alone; required fields unannounced.
- **Why:** placeholders vanish on input, color-only errors exclude color-blind users, and unannounced requirements fail form users (WCAG 3.3.1, 3.3.2, 1.4.1).
- **Fix:** explicit label with for/id on every input; error text linked via aria-describedby; required and invalid states exposed (required, aria-invalid) and announced in text, never color-only.

## Live Regions and Dialogs

- **Tell:** toasts, toggles, and status changes silent to screen readers; modals with no focus trap, focus lost on close, no Escape.
- **Why:** dynamic updates outside the focus are invisible without live regions; an untrapped dialog strands keyboard users (WCAG 4.1.3, 2.1.1).
- **Fix:** status changes go through an aria-live polite region or role=status; dialogs trap focus, return focus to the trigger on close, and close with Escape; loading spinners carry text, not just motion.

## Motion Safety

- **Tell:** infinite animation with no pause control; vestibular triggers (parallax, spin, zoom); reduced-motion ignored.
- **Why:** motion can sicken or harm; users must be able to stop it (WCAG 2.2.2, 2.3.3).
- **Fix:** honor prefers-reduced-motion (collapse to instant state changes); no auto-playing infinite motion without a pause control. Motion values themselves belong to the `animate` / gsap skills.
