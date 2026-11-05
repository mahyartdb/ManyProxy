# ManyProxy English copy. Keys are stable; the literal text is the product's
# current English wording and must not change during extraction.

app-title = ManyProxy
window-title-running = ManyProxy · { $running } running
running-suffix = · { $running } running

write-banner-cannot-write = Changes will not be saved — cannot write to the data folder ({ $folder }).
write-banner-cannot-use = Changes will not be saved — cannot use the data folder ({ $folder }).
write-banner-open-folder = Open data folder
write-banner-open-folder-tip = Open the folder that holds the library

# Title bar
titlebar-minimize = Minimize the window
titlebar-maximize = Maximize the window
titlebar-restore = Restore the window
titlebar-close = Close the window

# Tray
tray-show = Show
tray-select-exit = Select exit
tray-select-preset = Select preset
tray-start-all = Start all
tray-stop-all = Stop all
tray-open-config = Open config folder
tray-quit = Quit

# Toolbar
action-start-all = Start all
action-stop-all = Stop all
action-import = Import
action-add-config = + Add configuration
action-add-config-accessible = Add configuration
action-add-config-tip = Add a proxy configuration (Ctrl-N)
action-settings = Settings
action-command-palette = Command palette
toolbar-program = Program
toolbar-program-tip = App and single-config intake
toolbar-export = Export…
toolbar-export-tip = Export library to JSON
toolbar-about = About
toolbar-about-tip = Version, sidecars, shortcuts
toolbar-settings-tip = Settings
toolbar-library = Library
toolbar-library-tip = Subscriptions, groups, and cleanup
toolbar-subscriptions = Subscriptions…
toolbar-subscriptions-tip = Manage subscription lists
toolbar-groups = Groups…
toolbar-groups-tip = Rename or merge groups
toolbar-prune-duplicates-tip = Remove later copies in the focused group (asks first)
toolbar-prune-unavailable-tip = Remove dead rows in the focused group (asks first)
toolbar-prune-invalid-tip = Remove validation-failing rows in the focused group (asks first)
toolbar-prune-clear-tests-tip = Clear test results in the focused group (rows kept, asks first)
toolbar-routing = Routing
toolbar-routing-tip = Routing presets and settings
toolbar-routing-presets = Routing presets
toolbar-routing-presets-tip = Manage routing presets
toolbar-attach-preset = Attach preset to selected row
toolbar-attach-preset-tip = Pick a routing preset for the selected row
toolbar-open-settings-tip = Open Settings
toolbar-tools = Tools
toolbar-tools-tip = Selection, diagnostics, and commands
toolbar-start-all-tip = Start every configuration
toolbar-stop-all-tip = Stop every endpoint
toolbar-test-all = Test all
toolbar-test-all-tip = Test every running endpoint
toolbar-stop-testing = Stop Testing
toolbar-stop-testing-tip = Stop the running group test
toolbar-logs = Logs
toolbar-logs-tip = Sidecar output
toolbar-runtime-stats = Runtime stats
toolbar-runtime-stats-tip = Process health on one screen
toolbar-traffic-stats = Traffic stats
toolbar-traffic-stats-tip = Per-exit traffic with period and By Profile
toolbar-show-security = Show security
toolbar-show-security-tip = Toggle the transport-security column
toolbar-check-updates = Check for updates
toolbar-check-updates-tip = Fetch the release feed once (JSON only — nothing downloads or runs)
toolbar-row-columns = Row columns
toolbar-row-columns-tip = Choose which columns each library row shows
toolbar-palette-tip = Every action, searchable (Ctrl-K)
toolbar-import-tip = Import share links or files (Ctrl-O)
toolbar-connecting = Connecting…
toolbar-starting-tip = Starting every endpoint…

# Status strip
status-clear-search = Clear search
status-clear-search-tip = Clear the search query (keeps facet filters and checked)
status-palette-hint = Command palette (Ctrl+K)
status-count =
    { $total } { $total ->
        [one] configuration
       *[other] configurations
    } · { $enabled } set for launch
search-count = { $visible } of { $total }
engine-status-stopped = engine stopped
engine-status-running = engine running
engine-status-crashed = engine crashed ({ $cause })
health-never-tested = health never tested
health-count = health { $live } live · { $dead } dead
traffic-totals = out { $tx } · in { $rx }

# Bulk bar
bulk-tooltip = Blue wash = selected (F5). Blue bar + filled checkbox = checked (bulk).
bulk-start = Start
bulk-start-tip = Start every checked endpoint
bulk-stop = Stop
bulk-stop-tip = Stop every checked endpoint
bulk-more = More
bulk-more-aria = More bulk actions
bulk-more-tip = Move, copy, or delete the checked rows
bulk-check-visible = Check all visible
bulk-uncheck-all = Uncheck all
bulk-move = Move to group
bulk-copy-proxies = Copy proxies
bulk-delete = Delete
bulk-checked = { $checked } checked (of { $visible })
bulk-checked-selected = { $checked } checked · { $selected } selected (of { $visible })

# Library columns
column-group = Group
column-protocol = Protocol
column-upstream = Upstream
column-endpoint = Endpoint
column-health = Health
column-traffic = Traffic
column-name = Name
column-width-wider = Wider
column-width-narrower = Narrower
column-width-reset = Reset
column-width-aria = { $action } { $column } column
column-restored = { $column } column restored to default
columns-reset = Columns reset to defaults
column-layout-save-failed = Could not save column layout: { $error }
row-columns-save-failed = Could not save column choice: { $error }

# Sort headers and menus
sort-key-none = None (library order)
sort-key-latency = Latency (latest URL test)
sort-key-download = Download (current session)
sort-key-upload = Upload (current session)
sort-key-out-ip = Out-IP (text)
sort-key-traffic-total = Traffic total (counting window)
sort-key-traffic-down = Traffic down (counting window)
sort-key-traffic-up = Traffic up (counting window)
sort-key-type-security = Type | Security (fixed)
sort-direction-ascending = ascending
sort-direction-descending = descending
sort-marker-asc = · asc
sort-marker-desc = · desc
sort-marker-fixed = · fixed
sort-current-aria = { $label } — current sort
sort-by = Sort by { $label }
sort-direction = Direction: { $from } → { $to }
sort-direction-tip = Reverse the active key (Type | Security keeps its fixed order)
sort-header-active-tip = { $label } — sorted by { $key } ({ $direction }) — click to reverse, right-click to change
sort-header-idle-tip = { $label } — click to sort, right-click to pick a sort key

# Search and facet bar
facet-group-dimension = group or status
search-placeholder = Search configurations
search-hint = port=N or port=N:M filters by listen port (port=10801, port=10800:10900, port=10801:, port=:10900)
facet-group-aria = Filter by group or status
facet-latency-tip = Latest URL-test latency. Buckets are inclusive: ≤100 ms includes exactly 100 ms.
facet-latency-aria = Filter by probe latency
facet-include-unprobed = Include unprobed
facet-include-unprobed-tip = Keep rows that have not passed a URL test. Off means only measured latencies count.
facet-include-unprobed-aria = Include rows that have not been probed
facet-auto-clear = Auto-clear unavailable
facet-auto-clear-tip = Opt-in per group: after this group's URL test ends, its unavailable rows are removed automatically. Running rows and the row being edited are never removed; enabling asks its own confirmation.
facet-auto-clear-aria = Auto-clear unavailable rows in group "{ $group }" after group tests
facet-clear = Clear facet filters
facet-clear-tip = Clear the group, status, and latency filters (keeps search and checked)
facet-tip = Filter by { $dimension }: { $active } · { $visible } of { $total } shown
facet-all-groups = All groups ({ $total })
facet-group-all = Group: All
facet-group-choice = { $group } ({ $count })
facet-group-display = Group: { $group }
facet-status-choice = { $status } ({ $count })
facet-status-display = Status: { $status }
facet-latency-choice = Latency: { $bucket }
facet-all = All
facet-status-enabled = Enabled
facet-status-disabled = Disabled
facet-status-running = Running
facet-status-dead = Dead
facet-status-stopped = Stopped
latency-any = Any
latency-100 = ≤100 ms
latency-300 = ≤300 ms
latency-1000 = ≤1000 ms
filter-group = group: { $group }
filter-status = status: { $status }
filter-latency = latency: { $bucket }
filter-latency-probed = latency: { $bucket } (probed only)
filter-latency-probed-only = latency: probed only
filter-search = search: "{ $text }"

# Library rows and empty states
library-loading = Loading…
library-loading-subscriptions = Fetching subscriptions…
library-load-failed = Could not load configurations
library-retry = Retry
library-retry-tip = Fetch the subscriptions again
library-clear-all = Clear all
library-clear-all-tip = Clear search and every filter
library-no-matches = No matches — Clear search
library-empty-title = No proxy configurations yet
library-empty-text = Add one proxy, or use Import for many at once.
library-no-upstream = no upstream
row-checked-aria = Checked { $name } (bulk action target)
row-check-aria = Check { $name } for bulk actions
row-bind-tip = Serve bind address { $bind }
row-port-tip = Local endpoint { $bind }:{ $port } — edit it in the inspector
row-port-aria = Local port { $port } on { $bind }
row-copy-socks = Copy SOCKS
row-copy-socks-aria = Copy SOCKS URL for { $name }
row-copy-socks-tip = Copy this row's SOCKS proxy URL only — socks5://{ $bind }:{ $port }
row-lamp-active-tip = { $label } · click to stop filtering
row-lamp-idle-tip = { $label } · click to show only { $status } rows
lamp-failed = Failed — { $name } (dead while serving)
lamp-serving = Serving — { $name }
lamp-stopped = Stopped — { $name }

# Row context menus
rowctx-multi-start = Start { $count } configurations
rowctx-multi-stop = Stop { $count } configurations
rowctx-multi-test = Test { $count } configurations
rowctx-multi-copy-urls = Copy { $count } URLs
rowctx-multi-copy-proxy = Copy { $count } SOCKS URLs
rowctx-multi-move = Move { $count } configurations to group
rowctx-multi-delete = Delete { $count } configurations
rowctx-reset = Reset…
rowctx-more = More ▾
row-kebab-on = On
row-kebab-off = Off
row-kebab-aria = More actions
row-kebab-tip = More actions for the selected endpoint

# Routing prune verbs
prune-duplicates = Remove duplicates
prune-unavailable = Remove unavailable
prune-invalid = Remove invalid
prune-clear-tests = Clear tests
