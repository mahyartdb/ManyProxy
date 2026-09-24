# Contract — Import formats

## Accepted

- Share links (per-protocol URI form), JSON arrays of outbounds, v2rayN links, limited Shadowsocks/Clash host-port-credential imports.
- Subscription URLs over http(s) with manual refresh + optional auto refresh.

## Rules

- Paste/drag-drop/clipboard import creates disabled configs in the chosen group.
- Refresh upserts group members; user-edited fields win; failures preserve prior members and record the error on the subscription.
- Unsupported content is rejected with a reason + the offending item index (never partial silent import).
