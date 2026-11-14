# Endpoints

Every config serves on a local endpoint — a bind address and a port. The
Endpoint column shows it as `:port` on the default bind (`127.0.0.1`) and
`host:port` otherwise, with IPv6 addresses bracketed.

## Reading the chip

- **Click the chip** to copy the row's SOCKS URL to the clipboard.
- **Click the chevron** for SOCKS, HTTP, or both formats at once.
- **Hover and click the pencil** (or press `Ctrl+E`, or use `Edit` in the
  inspector) to change the endpoint.

## Editing

The editor validates as you type: the bind must be an IPv4/IPv6 address or
`localhost`, and the port must be free on that bind. When the port is taken the
editor offers the first free port on the same bind; when the bind would expose
the machine beyond loopback, a confirm names the config and the address before
anything starts.

- **Stopped row:** Apply writes the endpoint in place.
- **Running row:** Apply restarts the process on the new endpoint. If the
  restart fails, the store rolls back and the old endpoint keeps serving.

`Ctrl+Shift+E` opens the full config form instead, where the endpoint is one
field among the rest.
