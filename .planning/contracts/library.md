# Contract — Library management

## Actions

- `add-config(input)` → creates disabled ProxyConfig; invalid input rejected with field-level error.
- `edit-config(id, patch)` → edits stopped configs freely; editing a running config requires stop-first or applies on restart (explicit choice, no silent behavior).
- `remove-config(id)` → confirm dialog naming the config; removes its endpoint first, then the record.
- `set-enabled(id, bool)` / `start(id)` / `stop(id)` / `start-all` / `stop-all`.
- `copy-proxy-url(id)` → copies both `socks5://bind:port` and `http://bind:port` forms.

## Errors

- Duplicate port: names both configs. Unknown protocol: names the type + nearest supported. Missing extra-core binary: names the path.
