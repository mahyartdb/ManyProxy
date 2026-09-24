# Checklist shape (grafted from spec-kit `checklist-template.md`, adapted to `checklist`)

Mirrors this skill's Steps; adds the file shape and numbering discipline.

## File shape (`FEATURE_DIR/checklists/[domain].md`)

```markdown
# [Domain] Checklist: [Feature name]

**Purpose**: [what this checklist covers]
**Created**: [DATE]
**Feature**: [link to spec.md]
```

- Ownership note: `[x]` = reviewer judged the requirements-quality criterion satisfied, never implementation completion.
- `##` sections with `- [ ] CHK###` lines, numbered sequentially (`CHK001…`), continuing the last ID when appending. Never delete items.
- Item shape: question + dimension tag + traceability ref (`[Spec §X.Y]`, `[Gap]`, `[Ambiguity]`, `[Conflict]`, `[Assumption]`).
- Notes: mark `[x]` only after review confirms the criterion; `implement` reads checkbox state as a gate and must never modify markers.
