---
description: QSTACK steps menu — the loop 0 to 11 in order. Display only, no action.
---

## Run

Show the table below verbatim, then ask which step to run.

| #   | Step                   | Command                            | Status                 |
| --- | ---------------------- | ---------------------------------- | ---------------------- |
| 0   | Spike                  | `prototype` skill                  | Conditional            |
| 1   | Refine                 | `/refine`                          | Raw ideas              |
| 2   | Research               | `/research`                        | Conditional            |
| 3   | Kill-gate 1            | `/decide` pre-spec                 | Mandatory              |
| 4   | Specify                | `/spec`                            | Mandatory              |
| 5   | Plan / Check / Tickets | `/plan` `/checklist` `/to-tickets` | Plan+tickets mandatory |
| 6   | Kill-gate 2            | `/decide` post-tickets             | Mandatory              |
| 7   | Build                  | `/implement`                       | Mandatory              |
| 8   | Converge               | `/converge`                        | Mandatory              |
| 9   | Review                 | `/review`                          | Mandatory gate         |
| 10  | Finish                 | `/finish`                          | Mandatory              |
| 11  | Ship                   | `/ship`                            | Separate run           |

## Next

The picked step's own Next applies.

## Optional

- `/qmode` — whole loop. `/council`, `/debug` — lanes.
