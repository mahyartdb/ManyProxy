//! `AppView` itself: construction, restore, shared state and post-Root deferral.
//! Belongs: fields and lifecycle methods every other `impl AppView` block uses.
//! Not here: one feature's dialogs or render — those get their own `app/` file.
use crate::*;

use gpui_kit::base::VirtualListScrollHandle;
use gpui_kit::component::list::ListState;

/// Live completion count for the `Testing N…` toast and the taskbar progress
/// bar. Probes run on worker threads, so the sweep writes here and a UI poller
/// copies the snapshot into the ring at 10fps — no thread ever touches an
/// entity.
#[derive(Clone, Copy)]
struct ProbeProgress {
    done: usize,
    total: usize,
    failed: usize,
}

struct CountingProber<'a> {
    inner: &'a (dyn HealthProber + Sync),
    progress: Arc<std::sync::Mutex<ProbeProgress>>,
}

/// The group sweep reports done/total through its own callback; this wrapper
/// only adds the failure counter, so the taskbar can turn red for a partial
/// sweep without changing `test_group`'s progress shape.
struct FailureCountingProber<'a> {
    inner: &'a (dyn HealthProber + Sync),
    progress: Arc<std::sync::Mutex<ProbeProgress>>,
}

fn note_probe_complete(progress: &std::sync::Mutex<ProbeProgress>, failed: bool) {
    let mut state = progress.lock().unwrap_or_else(|poison| poison.into_inner());
    state.done = (state.done + 1).min(state.total);
    if failed {
        state.failed = (state.failed + 1).min(state.total);
    }
}

/// Group sweeps already count completion in their own callback, so this only
/// raises the failure flag.
fn note_probe_failure(progress: &std::sync::Mutex<ProbeProgress>) {
    let mut state = progress.lock().unwrap_or_else(|poison| poison.into_inner());
    state.failed = (state.failed + 1).min(state.total);
}

impl HealthProber for CountingProber<'_> {
    fn supports(&self, config: &ProxyConfig) -> bool {
        let supported = self.inner.supports(config);
        if !supported {
            note_probe_complete(&self.progress, false);
        }
        supported
    }

    fn url_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String> {
        let result = self.inner.url_test(config, bind, port, url);
        note_probe_complete(&self.progress, result.is_err());
        result
    }

    fn tcp_ping(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
    ) -> Result<ProbeSuccess, String> {
        let result = self.inner.tcp_ping(config, bind, port);
        note_probe_complete(&self.progress, result.is_err());
        result
    }
}

impl HealthProber for FailureCountingProber<'_> {
    fn supports(&self, config: &ProxyConfig) -> bool {
        self.inner.supports(config)
    }

    fn url_test(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
        url: &str,
    ) -> Result<ProbeSuccess, String> {
        let result = self.inner.url_test(config, bind, port, url);
        if result.is_err() {
            note_probe_failure(&self.progress);
        }
        result
    }

    fn tcp_ping(
        &self,
        config: &ProxyConfig,
        bind: &str,
        port: u16,
    ) -> Result<ProbeSuccess, String> {
        let result = self.inner.tcp_ping(config, bind, port);
        if result.is_err() {
            note_probe_failure(&self.progress);
        }
        result
    }
}

fn spawn_probe_progress_poller(
    toast: Option<ToastHandle>,
    progress: Arc<std::sync::Mutex<ProbeProgress>>,
    active: Arc<AtomicBool>,
    cx: &mut Context<AppView>,
) {
    let weak = cx.entity().downgrade();
    cx.spawn(async move |_entity, cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(120))
            .await;
        let snapshot = *progress.lock().unwrap_or_else(|poison| poison.into_inner());
        let running = active.load(std::sync::atomic::Ordering::SeqCst);
        let _ = weak.update(cx, |view, cx| {
            if let Some(toast) = toast {
                update_probe_progress(toast, snapshot.done, snapshot.total, cx);
            }
            view.set_taskbar_progress(snapshot.done, snapshot.total, snapshot.failed);
            if !running {
                view.clear_taskbar_progress();
            }
        });
        if !running {
            break;
        }
    })
    .detach();
}

/// F16: the refresh batch's progress counters and the in-flight set, together
/// so a refresh starts, reports and clears as one unit. `AppView` reads
/// `refresh_batch_progress` through it; nothing else may be added here without
/// belonging to that lifecycle.
#[derive(Default)]
pub(crate) struct RefreshBatch {
    pub(crate) total: usize,
    pub(crate) members: HashSet<String>,
    pub(crate) added: usize,
    pub(crate) updated: usize,
    pub(crate) removed: usize,
    pub(crate) failed: usize,
    pub(crate) conflicts: usize,
    pub(crate) in_use: usize,
    pub(crate) added_names: Vec<String>,
    pub(crate) updated_names: Vec<String>,
    pub(crate) removed_names: Vec<String>,
    pub(crate) in_use_names: Vec<String>,
    pub(crate) generation: u64,
    pub(crate) inflight: HashSet<String>,
    /// T-20: refreshes whose completion carries the named `[+]/[~]/[-]/[=]`
    /// report — set for manual origins only, so due-tick refreshes finish
    /// count-only.
    pub(crate) inflight_named: HashSet<String>,
    /// B16: the last fetch/store failure, held until the in-flight set drains
    /// so the list can report it when it has no rows to fall back on.
    pub(crate) last_error: Option<String>,
}

/// F16: everything the Logs sheet owns — the record store plus the filters,
/// tail/seen marks and the list/input entities the sheet rebuilds.
#[derive(Default)]
pub(crate) struct LogsState {
    pub(crate) store: LogStore,
    pub(crate) filter: String,
    pub(crate) exclude: String,
    pub(crate) regex: String,
    pub(crate) level: Option<LogLevel>,
    pub(crate) tail: bool,
    pub(crate) open: bool,
    pub(crate) seen: u64,
    pub(crate) inputs: Option<(Entity<InputState>, Entity<InputState>, Entity<InputState>)>,
    pub(crate) cap_input: Option<Entity<InputState>>,
    /// Captured just before a control-driven sheet swap. The live input text
    /// wins over the committed filter fields so typed-but-unapplied text
    /// survives the swap, then is consumed once on the next open.
    pub(crate) reopen_draft: Option<(String, String, String)>,
    pub(crate) list: Option<Entity<ListState<LogLinesDelegate>>>,
}

/// B3: one row-listing inspector pane's scroll window. `sizes` is the list's
/// caller-owned height cache: rows are uniform, so it only changes when the row
/// count does, and the list never needs a measure-then-correct pass beyond the
/// frame that infers its width. `scroll` is the memory that makes coming back
/// to a pane land where the user left it instead of at the top.
pub(crate) struct PaneWindow {
    sizes: Rc<Vec<Size<Pixels>>>,
    rows: usize,
    row_h: f32,
    pub(crate) scroll: VirtualListScrollHandle,
}

impl PaneWindow {
    pub(crate) fn new() -> Self {
        Self {
            sizes: Rc::new(Vec::new()),
            rows: 0,
            row_h: 0.,
            scroll: VirtualListScrollHandle::new(),
        }
    }

    pub(crate) fn sizes_for(&mut self, rows: usize, row_h: f32) -> Rc<Vec<Size<Pixels>>> {
        if self.rows != rows || self.row_h != row_h {
            self.rows = rows;
            self.row_h = row_h;
            // Only the row height matters on a vertical list; the width is
            // inferred from one measured row inside the element.
            self.sizes = Rc::new(vec![
                Size {
                    width: px(0.),
                    height: px(row_h)
                };
                rows
            ]);
        }
        self.sizes.clone()
    }
}

/// F16: the Traffic sheet's snapshot and its view state, replaced in place on
/// every poll.
#[derive(Default)]
pub(crate) struct ConnectionState {
    pub(crate) snapshot: Vec<(String, crate::health::ClashConn)>,
    /// T-25: counts derived from `snapshot` on the traffic tick, so the status
    /// bar never walks the snapshot per frame.
    pub(crate) counts: crate::health::ConnCounts,
    pub(crate) delta: HashMap<String, (u64, u64)>,
    pub(crate) selected_only: bool,
    pub(crate) sort_desc: bool,
    pub(crate) copied: Option<ConnCopyTarget>,
    pub(crate) copied_generation: u64,
    pub(crate) last_poll_at: i64,
}

impl AppView {
    /// Defect: boot runs inside `cx.open_window`'s closure, and the window only
    /// adopts its `gpui_component::Root` layer after that closure returns —
    /// `Root::new` does not install the first layer during it. A notification
    /// or dialog pushed there panics inside `Root::update` ("window first layer
    /// should be a gpui_component::Root"). Boot paths hand their overlay to
    /// these helpers, which land it on the first frame, after Root exists. The
    /// decision (message, severity, dialog content) is still taken at boot.
    pub(crate) fn after_root(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        open: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        cx.on_next_frame(window, move |view, window, cx| open(view, window, cx));
    }

    pub(crate) fn notify_after_root(
        &mut self,
        notification: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.after_root(window, cx, move |_view, window, cx| {
            window.push_notification(notification, cx);
        });
    }

    /// Whether the window has adopted its `gpui_component::Root` first layer
    /// yet. `Root::update` - which every `WindowExt` overlay call funnels
    /// through - does `window.root::<Root>().flatten().expect(..)`, so this is
    /// exactly the panic condition: it is false while `cx.open_window`'s
    /// closure is still running, and also for a window whose root view is not
    /// a `Root` at all (the test harness shape).
    pub(crate) fn root_installed(window: &Window) -> bool {
        window.root::<Root>().flatten().is_some()
    }

    /// A path that boots *and* runs later (`reload`): deliver now when the
    /// window already has its Root, so ordinary use keeps its exact timing,
    /// and hand the notification to the first frame during boot.
    pub(crate) fn notify_now_or_after_root(
        &mut self,
        notification: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Self::root_installed(window) {
            window.push_notification(notification, cx);
        } else {
            self.notify_after_root(notification, window, cx);
        }
    }

    pub(crate) fn with_store_inner(
        window: &mut Window,
        cx: &mut Context<Self>,
        store: Store,
        fallback: Option<WriteFallback>,
    ) -> Self {
        let mut store = store;
        // The locale seam is reserved for a future language picker. Reading it
        // before the first copy is built keeps this session's strings coherent.
        let locale_override = store.get_meta(crate::i18n::LOCALE_META_KEY).ok().flatten();
        crate::i18n::set_locale(&crate::i18n::resolve_locale(locale_override.as_deref()));
        // T-34: read the previous session's verdict first, then assume this
        // session is dirty. The write lands before any restore logic, so a
        // kill/crash leaves "0" (unclean) for the next boot.
        let previous_clean_exit = store.last_clean_exit();
        let last_snapshot = store.load_running_snapshot();
        if let Err(error) = store.set_last_clean_exit(false) {
            eprintln!("clean-exit marker reset failed: {error}");
        }
        let unclean_notice = unclean_exit_notice(previous_clean_exit, &last_snapshot);
        // T-33: the first read seeds the address list once (constants + every
        // row's current bind); every later read is the stored extras alone.
        let bind_hosts = store.bind_hosts();
        // T-04: the probe budget is the persisted one, not the built-in default.
        let probe_timeout_secs = store.probe_timeout_secs();
        let libraries = cx.new(|cx| {
            ListState::new(LibraryDelegate::new(), window, cx)
                .searchable(false)
                .selectable(true)
        });
        let last_query = store.library_last_query().unwrap_or_default();
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(crate::search_placeholder())
                .default_value(last_query.clone())
        });
        let row_columns = RowColumns::parse(&store.library_columns_raw().unwrap_or_default());
        let layout = store.library_column_layout();
        let sort = store.library_sort();
        let latency_facet = store.latency_facet();
        libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_row_columns(row_columns);
            state.delegate_mut().set_layout(layout);
            state.delegate_mut().set_sort(sort);
            state.delegate_mut().set_latency_facet(latency_facet);
            state.delegate_mut().query = last_query;
            state.delegate_mut().set_bind_hosts(bind_hosts);
            list_cx.notify();
        });

        let mut empty_delegate = LibraryDelegate::new();
        empty_delegate.set_latency_facet(latency_facet);
        let (choices, current) = FacetMenu::Group.choices(&empty_delegate);
        let facet_group = FacetPicker::new(FacetMenu::Group, choices, current, window, cx);
        let (latency_choices, latency_current) = FacetMenu::Latency.choices(&empty_delegate);
        let facet_latency = FacetPicker::new(
            FacetMenu::Latency,
            latency_choices,
            latency_current,
            window,
            cx,
        );

        apply_appearance(&store.appearance(), store.accent(), Some(window), cx);
        apply_text_size(store.text_size(), cx);
        crate::os::window::apply_corner_preference(window, store.window_corners());
        // K5: the toast park rides `apply_appearance` (bottom-right above the
        // status bar, max 2) so it survives every theme change for free.

        // The sweep shells out to netstat/tasklist; the suite mounts hundreds of
        // views and proves nothing by reaping the runner's process table. The
        // sweep itself is unit-tested with injected probes.
        #[cfg(not(test))]
        {
            let our_ports: Vec<u16> = store
                .list()
                .map(|rows| rows.iter().map(|row| row.port).collect())
                .unwrap_or_default();
            crate::engine::production_startup_reap(&our_ports);
        }

        let sidecar_presence = [
            (engine_binary_file_name(), resolve_engine_binary().is_ok()),
            (fork_binary_file_name(), resolve_fork_binary().is_ok()),
            (xray_binary_file_name(), resolve_xray_binary().is_ok()),
        ]
        .into_iter()
        .map(|(file, found)| sidecar_presence_line(&file, found))
        .collect::<Vec<_>>();
        let log_cap = store.log_max_lines();
        // T-08: the first honest question after the store opens is whether its
        // folder accepts a write — a read-only folder reads fine and lets a
        // save evaporate. Probe once here; the tick and every failed save
        // re-probe the same state. A session that fell back to memory has no
        // folder to probe, so its open failure is the standing cause instead.
        let (write_blocked, write_blocked_folder, write_blocked_pinned) = match fallback {
            Some(fallback) => (Some(fallback.reason), fallback.folder, true),
            None => (store.write_probe().err(), None, false),
        };
        let split_state = cx.new(|_| ResizableState::default());
        let view = Self {
            focus: cx.focus_handle(),
            libraries: libraries.clone(),
            detail_scroll: ScrollHandle::new(),
            split_state,
            inspector_visible: false,
            pane_logs: PaneWindow::new(),
            pane_links: PaneWindow::new(),
            pane_traffic: PaneWindow::new(),
            store,
            write_blocked,
            write_blocked_folder,
            write_blocked_pinned,
            engine: EngineController::production(),
            health_store: HealthStore::default(),
            logs: LogsState {
                store: LogStore::with_capacity(log_cap),
                tail: true,
                ..LogsState::default()
            },
            settings_subscriptions: Vec::new(),
            settings_last_check: None,
            expanded_subs: HashSet::new(),
            out_ip_by_row: HashMap::new(),
            traffic_last_seen: TrafficSeenMap::default(),
            traffic_tick_in_flight: false,
            traffic_mirror_hydrated: false,
            prober: Box::new(RealHealthProber::new(std::time::Duration::from_secs(
                probe_timeout_secs,
            ))),
            ticks_enabled: true,
            refresh: RefreshBatch::default(),
            probe_url_in_flight: false,
            probe_ping_in_flight: false,

            update_check_in_flight: false,
            update_available: None,
            passive_update_epoch: 0,
            probe_stop: Arc::new(AtomicBool::new(false)),
            group_test_in_flight: false,
            probe_toast_key: next_toast_key(),
            subs_form: None,
            preset_form: None,
            import_form: None,
            edit_form_open: None,
            rule_set_download_in_flight: false,
            import_parse_in_flight: false,
            import_parse_epoch: 0,
            import_cancel_epoch: None,
            close_confirmed: false,
            tray: None,
            window_was_maximized: false,
            os_taskbar: None,
            os_notifier: None,
            os_notice_policy: crate::os::notify::NoticePolicy::default(),
            last_snapshot,
            start_in_flight: false,
            ticks_started: false,
            sidecar_presence,
            _libraries_subscription: cx.subscribe(
                &libraries,
                |_view, _state, _event: &ListEvent, cx| {
                    cx.notify();
                },
            ),
            search_input: search_input.clone(),
            _search_subscription: cx.subscribe(
                &search_input,
                |view: &mut Self, emitter: Entity<InputState>, event: &InputEvent, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let query = emitter.read(cx).value().to_string();
                    view.remember_library_query(&query);
                    view.libraries.update(cx, |state, list_cx| {
                        let delegate = state.delegate_mut();
                        delegate.query = query;
                        delegate.apply_filter();
                        list_cx.notify();
                    });
                    cx.notify();
                },
            ),
            facet_group,
            facet_latency,
            row_columns,
            column_resize: None,
            column_drop: None,
            inspector_tab: InspectorTab::Details,
            conn: ConnectionState {
                selected_only: true,
                sort_desc: true,
                ..ConnectionState::default()
            },
            palette: None,
            traffic_period: store::TrafficPeriod::Day,
            shard_rings: BTreeMap::new(),
            shard_baselines: BTreeMap::new(),
            shard_denied: HashSet::new(),
            process_sampler: Box::new(win32_process_sample),
            started_at: std::time::Instant::now(),
        };
        let mut view = view;
        // T-06: publish the shell's focus handle once per window so every
        // overlay-dismiss path can reclaim keyboard focus without threading
        // the handle through each dialog builder and footer closure.
        cx.set_global(ShellFocus {
            window: window.window_handle().window_id(),
            handle: view.focus.clone(),
        });
        if let Some(notice) = unclean_notice {
            // The notification layer needs the Root view, which the window only
            // adopts after this constructor returns: the decision was taken
            // above at boot, the toast lands on the first frame — once.
            view.notify_after_root(notice, window, cx);
        }
        view.apply_engine_auto_restart();
        let rows = match view.store.list() {
            Ok(rows) => rows,

            Err(error) => {
                // Boot path (this constructor runs inside `cx.open_window`'s
                // closure): the window has no Root yet, so the toast lands on
                // the first frame instead of panicking in `Root::update`.
                view.notify_after_root(
                    Notification::error(db_list_error_message(&error)),
                    window,
                    cx,
                );
                Vec::new()
            }
        };

        if let Ok(snapshots) = view.store.load_health_snapshots() {
            let known: HashSet<String> = rows.iter().map(|row| row.id.clone()).collect();
            let kept: Vec<HealthSnapshot> = snapshots
                .into_iter()
                .filter(|snapshot| known.contains(&snapshot.config_id))
                .collect();
            view.health_store.hydrate(&kept);
        }
        libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_rows(rows);

            list_cx.notify();
        });

        view.sync_facet_pickers(window, cx);
        cx.notify();
        view
    }

    pub(crate) fn config_count(&self, cx: &App) -> usize {
        self.libraries.read(cx).delegate().counts().0
    }

    pub(crate) fn selected_config(&self, cx: &App) -> Option<ProxyConfig> {
        self.libraries.read(cx).delegate().selected_row()
    }

    pub(crate) fn cached_upstream(&self, cx: &App, config: &ProxyConfig) -> String {
        self.libraries
            .read(cx)
            .delegate()
            .upstream_for(&config.id)
            .map(str::to_string)
            .unwrap_or_else(|| upstream_host_port(config).unwrap_or_else(|| t!("no-upstream")))
    }

    /// T-08: one probe, one state write. Returns true when the standing
    /// condition changed, so the tick only asks for a repaint when there is
    /// something new to paint.
    pub(crate) fn refresh_write_blocked(&mut self, cx: &mut Context<Self>) -> bool {
        // T-08: a failed open pins the banner. A probe can prove the folder
        // writable again, but this session is still in memory: clearing the
        // banner would promise saves that still cannot happen. Only a restart
        // that opens the file store clears it.
        if self.write_blocked_pinned {
            return false;
        }
        let blocked = self.store.write_probe().err();
        if blocked == self.write_blocked {
            return false;
        }
        self.write_blocked = blocked;
        cx.notify();
        true
    }

    /// T-08: a failed save may be the folder, not the row. The probe is the
    /// honest arbiter: a folder that refuses a write gets the standing banner
    /// (a toast would vanish); any other failure keeps its own honest toast.
    pub(crate) fn report_save_error(
        &mut self,
        error: StoreError,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(reason) = self.store.write_probe() {
            self.write_blocked = Some(reason);
            cx.notify();
            return;
        }
        window.push_notification(
            Notification::error(t!("cannot-save", error = error.to_string())),
            cx,
        );
    }

    pub(crate) fn list_or_notify(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Vec<ProxyConfig>> {
        match self.store.list() {
            Ok(rows) => Some(rows),
            Err(error) => {
                window.push_notification(Notification::error(db_list_error_message(&error)), cx);
                None
            }
        }
    }

    pub(crate) fn running_rows_or_notify(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Vec<ProxyConfig>> {
        let running = self.engine.running_ids();
        self.list_or_notify(window, cx).map(|rows| {
            rows.into_iter()
                .filter(|row| running.contains(&row.id))
                .collect()
        })
    }

    pub(crate) fn format_dialog_params(
        &mut self,
        params: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = params.read(cx).value().to_string();
        if current.trim().is_empty() {
            window.push_notification(Notification::info(t!("params-blank-info")), cx);
            return;
        }
        match format_params_json(&current) {
            Ok(pretty) => {
                params.update(cx, |state, cx| {
                    state.set_value(pretty, window, cx);
                });
            }
            Err(reason) => {
                window.push_notification(Notification::error(reason), cx);
            }
        }
    }

    pub(crate) fn template_dialog_params(
        &mut self,
        params: &Entity<TextareaState>,
        form: &ConfigForm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = form.protocol_text(cx);
        let template = protocol_params_template(&name);
        params.update(cx, |state, cx| {
            state.set_value(template, window, cx);
        });
        let shown = name.trim();
        let template_name = if shown.is_empty() {
            t!("template-default")
        } else {
            shown.to_string()
        };
        window.push_notification(
            Notification::info(t!("loaded-template", name = template_name)),
            cx,
        );
    }

    pub(crate) fn validate_dialog_params(
        &mut self,
        params: &Entity<TextareaState>,
        protocol: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let current = params.read(cx).value().to_string();
        match validate_params_json(&current) {
            Ok(()) => {}
            Err(reason) => {
                window.push_notification(Notification::error(reason), cx);
                return false;
            }
        }

        if protocol == "extra-core" {
            if let Err(reason) = validate_extra_params(&current) {
                window.push_notification(Notification::error(reason), cx);
                return false;
            }
        }
        window.push_notification(
            Notification::success(if current.trim().is_empty() {
                t!("params-blank-success")
            } else {
                t!("params-valid-json")
            }),
            cx,
        );
        true
    }

    pub(crate) fn row_config(&self, id: &str) -> Option<ProxyConfig> {
        self.store.get(id).ok().flatten()
    }

    /// Surfaces at most one engine-crash notification and drives the auto-restart
    /// policy. The pending cause lives in the controller, so this still sees a
    /// crash the 5 s tick already drained from the backend.
    pub(crate) fn service_engine_crash(&mut self) -> Option<Notification> {
        let now = std::time::Instant::now();
        let detail = self.engine.pending_crash();
        self.engine.settle_restart_budget(now);
        if self.engine.auto_restart_due(now) {
            self.engine.note_auto_restart_attempt(now);
            let notification = match self.engine.restart_crashed() {
                Ok(()) => Notification::success(t!(
                    "engine-recovered",
                    detail = detail.unwrap_or_else(|| t!("engine-recovered-fallback"))
                )),
                Err(error) => Notification::error(t!(
                    "engine-autorestart-failed",
                    error = redact_secrets(&error.to_string())
                )),
            };
            self.engine.clear_pending_crash();
            return Some(notification);
        }
        let detail = detail?;
        self.engine.clear_pending_crash();
        let text = crash_notification_text(&detail);
        self.os_notify(
            crate::os::notify::NoticeKind::EngineCrash,
            &t!("engine-crashed-title"),
            &redact_secrets(&text),
        );
        Some(Notification::error(text))
    }

    pub(crate) fn notify_engine_crash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(notification) = self.service_engine_crash() {
            // Reachable from `reload`, which the boot path calls before the
            // window adopts its Root layer — deliver it the same safe way.
            self.notify_now_or_after_root(notification, window, cx);
        }
    }

    pub(crate) fn push_engine_notification(
        &self,
        notification: Notification,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = cx.active_window() {
            let _ = handle.update(cx, move |_, window, cx| {
                window.push_notification(notification, cx);
            });
        }
    }

    pub(crate) fn warn_for_lan_binds(&self, window: &mut Window, cx: &mut App) {
        for warning in self.engine.lan_warnings() {
            window.push_notification(Notification::warning(warning), cx);
        }
    }

    pub(crate) fn running_names_for_close(&self) -> Vec<String> {
        let running = self.engine.running_ids();
        if running.is_empty() {
            return Vec::new();
        }
        match self.store.list() {
            Ok(rows) => rows
                .iter()
                .filter(|row| running.contains(&row.id))
                .map(|row| row.name.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub(crate) fn close_veto_text(&self) -> Option<String> {
        if self.close_confirmed {
            return None;
        }
        close_confirm_description(&self.running_names_for_close())
    }

    pub(crate) fn confirm_close_stop(&mut self, window: &Window) {
        self.engine.stop_all();
        if let Ok(rows) = self.store.list() {
            for row in &rows {
                if let Err(error) = self.store.set_enabled(&row.id, false) {
                    eprintln!("close flag clear failed for {}: {error}", row.id);
                }
            }
        }
        self.snapshot_running_set(&[]);
        self.mark_clean_exit();
        self.save_window_geometry(window);
        self.close_confirmed = true;
    }

    /// T-34: every orderly exit writes the marker right after its snapshot
    /// clear. A kill/crash never reaches here, so boot's "0" stands.
    pub(crate) fn mark_clean_exit(&mut self) {
        if let Err(error) = self.store.set_last_clean_exit(true) {
            eprintln!("clean-exit marker failed: {error}");
        }
    }

    /// T-26: mirrors the running count into the OS window title on the exact
    /// cadence the tray tooltip is fed from — no extra poll or timer. The
    /// custom titlebar label composes the same `window_title_text` at render.
    pub(crate) fn sync_running_title(&self, running: usize, cx: &mut Context<Self>) {
        let title = window_title_text(running);
        if let Some(handle) = cx.active_window() {
            let _ = handle.update(cx, move |_, window, _| window.set_window_title(&title));
        }
    }

    pub(crate) fn hide_to_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(windows)]
        match crate::tray::hwnd_from_window(window) {
            Some(hwnd) => {
                crate::tray::hide_window_handle(hwnd);
            }

            None => window.minimize_window(),
        }
        #[cfg(not(windows))]
        window.minimize_window();
        if let Some(tray) = &self.tray {
            tray.set_running(self.engine.running_count());
        }
        cx.notify();
    }

    pub(crate) fn show_window_from_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(windows)]
        if let Some(hwnd) = crate::tray::hwnd_from_window(window) {
            crate::tray::show_window_handle(hwnd);
        }
        window.activate_window();
        cx.notify();
    }

    pub(crate) fn quit_from_tray(&mut self, window: &Window) {
        self.engine.stop_all();
        if let Ok(rows) = self.store.list() {
            for row in &rows {
                if let Err(error) = self.store.set_enabled(&row.id, false) {
                    eprintln!("tray quit flag clear failed for {}: {error}", row.id);
                }
            }
        }
        self.snapshot_running_set(&[]);
        self.mark_clean_exit();
        self.save_window_geometry(window);

        self.tray = None;
    }

    /// A manual check: GET the JSON feed once, then surface the compare result.
    /// The passive weekly check (`start_passive_update_check`) is the only
    /// automatic path. It is opt-out in Settings -> Updates, reads the same
    /// feed at most once every seven days, and only posts the status-bar hint.
    /// Neither path downloads or runs a release; the feed read is the whole
    /// check.
    pub(crate) fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A manual check replaces whatever the passive path was about to say.
        self.passive_update_epoch = self.passive_update_epoch.wrapping_add(1);
        self.update_available = None;
        if self.update_check_in_flight {
            window.push_notification(Notification::info(t!("update-check-running")), cx);
            return;
        }
        let configured = self
            .store
            .update_feed_url()
            .filter(|value| !value.trim().is_empty());
        let feed_url = effective_update_feed_url(configured.as_deref(), APP_VERSION);
        self.update_check_in_flight = true;
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let fetched = fetch_release_feed(client, &feed_url, resolve_subscription_host).await;
            cx.update(|cx| {
                if let Some(handle) = cx.active_window() {
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.finish_update_check(fetched, window, cx);
                        });
                    });
                } else {
                    let _ = weak.update(cx, |view, cx| {
                        view.finish_update_check_no_window(fetched, cx);
                    });
                }
            });
        })
        .detach();
    }

    /// The passive weekly check: at most one feed read per seven days, decided
    /// at launch. The timestamp lands before the request so a failed attempt
    /// still waits out the week; a background failure is silent because the
    /// manual check owns error reporting.
    pub(crate) fn start_passive_update_check(&mut self, cx: &mut Context<Self>) {
        let enabled = self.passive_update_check_enabled();
        let last = self.store.last_update_check();
        let due = passive_update_check_due(last, crate::store::now_unix(), enabled);
        if !due {
            return;
        }
        self.passive_update_epoch = self.passive_update_epoch.wrapping_add(1);
        let epoch = self.passive_update_epoch;
        self.record_update_check();
        let configured = self
            .store
            .update_feed_url()
            .filter(|value| !value.trim().is_empty());
        let feed_url = effective_update_feed_url(configured.as_deref(), APP_VERSION);
        let client = cx.http_client();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let fetched =
                fetch_passive_update(due, client, &feed_url, resolve_subscription_host).await;
            let Some(fetched) = fetched else {
                return;
            };
            cx.update(|cx| {
                let _ = weak.update(cx, |view, cx| {
                    view.finish_passive_update_check(fetched, epoch, cx);
                });
            });
        })
        .detach();
    }

    /// T-04: writes the check's timestamp into the store and mirrors it into
    /// the open Settings dialog.
    pub(crate) fn record_update_check(&mut self) {
        let now = crate::store::now_unix();
        if let Err(error) = self.store.set_last_update_check(now) {
            eprintln!("update-check timestamp failed: {error}");
        }
        if let Some(cell) = &self.settings_last_check {
            *cell.borrow_mut() = Some(now);
        }
    }

    pub(crate) fn palette_op_at(index: IndexPath) -> Option<&'static str> {
        palette_action_at(index)
    }

    pub(crate) fn run_palette_op(
        &mut self,
        index: IndexPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = Self::palette_op_at(index) else {
            return;
        };
        window.close_all_dialogs(cx);
        run_action(self, id, ActionTarget::Selection, window, cx);
        // T-06: the palette's second overlay opens first; the reclaim runs last,
        // and its predicate leaves the new overlay's focus alone.
        reclaim_shell_focus(window, cx);
    }

    /// T-27: the latest URL-test sample per row, in ms. Dead samples (an error
    /// with no latency), ping-only evidence and never-tested rows are absent —
    /// which is what pins them last when `latency` sorts.
    pub(crate) fn latency_map(&self, rows: &[ProxyConfig]) -> HashMap<String, u64> {
        let mut map = HashMap::new();
        for row in rows {
            let Some(sample) = self.health_store.latest_of(&row.id, HealthKind::UrlTest) else {
                continue;
            };
            if sample.error.is_some() {
                continue;
            }
            if let Some(latency_ms) = sample.latency_ms {
                map.insert(row.id.clone(), latency_ms);
            }
        }
        map
    }

    /// T-27: the per-row session traffic mirror the traffic tick already keeps
    /// (`download`/`upload` sort keys).
    pub(crate) fn data_traffic_map(&self, rows: &[ProxyConfig]) -> HashMap<String, (u64, u64)> {
        rows.iter()
            .filter_map(|row| {
                self.health_store
                    .data_traffic(&row.id)
                    .map(|totals| (row.id.clone(), totals))
            })
            .collect()
    }

    /// T-25: `N active · M TCP · K UDP` from the last merged clash snapshot.
    /// Hidden while no core runs — there is no collection to report, and zeros
    /// would read as a measured idle instead of "nothing is being polled".
    /// K12/A5-1: the strip no longer renders this count; the Links
    /// inspector tab carries it. Retained for the counter merge tests only.
    #[cfg(test)]
    pub(crate) fn conn_counter_text(&self) -> Option<String> {
        if self.engine.running_count() == 0 {
            return None;
        }
        Some(self.conn.counts.status_text())
    }

    pub(crate) fn record_and_persist(&mut self, outcomes: &[TestOutcome]) {
        let mut snapshots = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            self.health_store.record_outcome(outcome);
            if outcome_is_neutral(outcome) {
                // A neutral probe never became a health sample; keeping it out
                // of the snapshots stops hydrate from reviving it as latest.
                continue;
            }
            snapshots.push(HealthSnapshot {
                config_id: outcome.sample.config_id.clone(),
                at: outcome.sample.at,
                kind: outcome.sample.kind.as_str().to_string(),
                latency_ms: outcome.sample.latency_ms,
                error: outcome.sample.error.clone(),
            });
        }
        if !snapshots.is_empty() {
            if let Err(error) = self.store.save_health_snapshots(&snapshots) {
                eprintln!("health snapshot persist failed: {error}");
            }
        }
    }

    pub(crate) fn group_running_rows(&self, group: &str) -> Vec<ProxyConfig> {
        let running = self.engine.running_ids();
        self.store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == *group && running.contains(&row.id))
            .collect()
    }

    pub(crate) fn spawn_group_probe(
        &mut self,
        rows: Vec<ProxyConfig>,
        group: String,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        use std::sync::atomic::Ordering;
        if rows.is_empty() || self.group_test_in_flight {
            return;
        }
        let count = rows.len();
        let url = self.effective_test_url(&rows);
        self.group_test_in_flight = true;
        self.probe_stop.store(false, Ordering::SeqCst);
        let progress = Arc::new(std::sync::Mutex::new(ProbeProgress {
            done: 0,
            total: count,
            failed: 0,
        }));
        let active = Arc::new(AtomicBool::new(true));
        let probe_key = self.probe_toast_key;
        let probe_toast = window.and_then(|handle| {
            handle
                .update(cx, |_, window, cx| {
                    push_probe_start_toast(
                        probe_key,
                        probe_start_toast(count),
                        0,
                        count,
                        window,
                        cx,
                    )
                })
                .ok()
        });
        if window.is_some() {
            spawn_probe_progress_poller(probe_toast, progress.clone(), active.clone(), cx);
        }
        let stop = self.probe_stop.clone();
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();
        let retry_group = group.clone();
        cx.spawn(async move |_entity, cx| {
            let sweep_progress = progress;
            let sweep_active = active;
            let (outcomes, prober, stopped) = cx
                .background_executor()
                .spawn(async move {
                    let prober = prober;
                    let counting = FailureCountingProber {
                        inner: &*prober,
                        progress: sweep_progress.clone(),
                    };
                    let outcomes = test_group(&rows, &url, &counting, &stop, move |done, total| {
                        let mut state = sweep_progress
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner());
                        state.done = done;
                        state.total = total;
                    });
                    let stopped = stop.load(Ordering::SeqCst);
                    sweep_active.store(false, Ordering::SeqCst);
                    (outcomes, prober, stopped)
                })
                .await;
            cx.update(|cx| {
                let recorded = weak.update(cx, |view, _| {
                    view.prober = prober;
                    view.group_test_in_flight = false;
                    view.record_and_persist(&outcomes);
                    // T-19: the sweep's outcomes are already persisted by
                    // record_and_persist, so auto-clear consumes them through
                    // the same snapshots the manual prune reads — no second
                    // probe pass lives anywhere.
                    let cleared = view.auto_clear_after_sweep(&group);
                    let (live, dead) = view.health_counts();
                    let sweep_dead = outcomes
                        .iter()
                        .filter(|outcome| outcome.sample.latency_ms.is_none())
                        .count();
                    (live, dead, cleared, sweep_dead)
                });
                let Ok((live, dead, auto_cleared, sweep_dead)) = recorded else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };
                let message = group_test_summary_toast(&group, count, live, dead, stopped);
                let auto_text = auto_cleared
                    .as_ref()
                    .map(|(removed, kept)| auto_clear_summary_toast(&group, *removed, kept));
                let auto_success = auto_cleared
                    .as_ref()
                    .is_some_and(|(removed, _)| *removed > 0);
                if let Some(handle) = window {
                    if handle
                        .update(cx, |_, window, cx| {
                            view.update(cx, |view, cx| {
                                view.reload(window, cx);
                                // K5: the start toast leaves exactly when the
                                // sweep's own summary arrives.
                                if let Some(toast) = probe_toast {
                                    close_probe_start_toast(toast, window, cx);
                                }
                                let mut summary = if sweep_dead > 0 {
                                    ToastSpec::warning(message)
                                } else {
                                    ToastSpec::success(message)
                                };
                                if sweep_dead > 0 {
                                    let retry_view = cx.entity().downgrade();
                                    let retry_group = retry_group.clone();
                                    summary = summary.action(
                                        "toast-retry-test",
                                        t!("toast-action-retry"),
                                        move |window, cx| {
                                            let _ = retry_view.update(cx, |view, cx| {
                                                view.retry_group_probe(&retry_group, window, cx);
                                            });
                                        },
                                    );
                                }
                                view.push_toast(summary, window, cx);
                                // T-19: exactly ONE auto-clear summary per
                                // sweep — success when rows went, info when
                                // every candidate was protected.
                                if let Some(auto_text) = auto_text {
                                    let auto = if auto_success {
                                        Notification::success(auto_text)
                                    } else {
                                        Notification::info(auto_text)
                                    };
                                    window.push_notification(auto, cx);
                                }
                            });
                        })
                        .is_ok()
                    {
                        return;
                    }
                }
                view.update(cx, |view, cx| view.reload_quiet(cx));
            });
        })
        .detach();
    }

    /// The retired Library menu's prune family. Slice 7 re-homes it in the
    /// group and source manage sheet.
    #[allow(dead_code)]
    pub(crate) fn group_test_sample_count(&self, group: &str) -> usize {
        let ids: HashSet<String> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|row| row.group == group)
            .map(|row| row.id)
            .collect();
        self.store
            .load_health_snapshots()
            .unwrap_or_default()
            .iter()
            .filter(|snap| ids.contains(&snap.config_id))
            .count()
    }

    /// T-19: end-of-group-test-sweep application. Runs after the sweep has
    /// persisted its outcomes, so candidates come from the same health
    /// snapshots the manual "Unavailable" prune reads — no second probe. The
    /// ticket's protected classes (running, row under edit, in-flight refresh
    /// batch) are excluded at the candidate-filter level; removal goes through
    /// the shared removal pipeline. Returns one (removed, kept-reasons) pair
    /// when a summary is warranted.
    pub(crate) fn auto_clear_after_sweep(&mut self, group: &str) -> Option<(usize, Vec<String>)> {
        if !self.store.auto_clear_enabled(group) {
            return None;
        }
        let dead = match self.store.unavailable_rows_in_group(group) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("auto-clear read failed: {error}");
                return None;
            }
        };
        if dead.is_empty() {
            return None;
        }
        let editing = self.edit_form_open.clone();
        let batch_members = self.refresh.members.clone();
        let running: HashSet<String> = self.engine.running_ids().into_iter().collect();
        let mut skip = HashSet::new();
        let mut reasons_seen: HashSet<&'static str> = HashSet::new();
        for row in dead {
            let reason = if running.contains(&row.id) {
                Some("running")
            } else if editing.as_deref() == Some(row.id.as_str()) {
                Some("editing")
            } else if row
                .subscription_id
                .as_deref()
                .is_some_and(|subscription| batch_members.contains(subscription))
            {
                Some("in-flight batch")
            } else {
                None
            };
            if let Some(name) = reason {
                skip.insert(row.id.clone());
                reasons_seen.insert(name);
            }
        }
        let removed = match self.store.remove_unavailable_in_group(group, &skip) {
            Ok(rows) => rows.len(),
            Err(error) => {
                eprintln!("auto-clear removal failed: {error}");
                return None;
            }
        };
        let kept: Vec<String> = ["running", "editing", "in-flight batch"]
            .iter()
            .filter(|reason| reasons_seen.contains(**reason))
            .map(|reason| match *reason {
                "running" => t!("kept-reason-running"),
                "editing" => t!("kept-reason-editing"),
                _ => t!("kept-reason-in-flight-batch"),
            })
            .collect();
        if removed == 0 && kept.is_empty() {
            return None;
        }
        Some((removed, kept))
    }

    /// T-27: one writer for the sort model — persists the meta row beside
    /// `library_columns`, mirrors the choice to the list, and says which order
    /// is live (the retired toggle's notification, now covering every key).
    pub(crate) fn set_library_sort(
        &mut self,
        key: LibrarySortKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let direction = self.libraries.read(cx).delegate().sort.direction;
        self.commit_library_sort(LibrarySort::new(key, direction), window, cx);
    }

    pub(crate) fn ping_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.row_config(id) else {
            window.push_notification(Notification::warning(t!("already-removed")), cx);
            return;
        };
        if !self.engine.running_ids().contains(&config.id) {
            window.push_notification(
                Notification::warning(t!("row-not-running", name = config.name)),
                cx,
            );
            return;
        }
        if self.probe_kind_in_flight(true) {
            window.push_notification(Notification::info(t!("health-test-running")), cx);
            return;
        }
        self.spawn_probe(vec![config], true, false, None, cx);
    }

    pub(crate) fn effective_test_url(&self, rows: &[ProxyConfig]) -> String {
        let global = self.store.test_url();
        if rows.len() == 1 {
            let row_override = self.store.row_test_url(&rows[0].id);
            return effective_probe_url(row_override.as_deref(), global.as_deref());
        }
        effective_probe_url(None, global.as_deref())
    }

    /// K5: Retry from a test-result toast. Rows are re-resolved because the
    /// library may have changed while the toast sat there; stopped or removed
    /// rows simply drop out of the retry.
    pub(crate) fn retry_probe(
        &mut self,
        ids: Vec<String>,
        ping: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let running = self.engine.running_ids();
        let rows: Vec<ProxyConfig> = ids
            .iter()
            .filter_map(|id| self.row_config(id))
            .filter(|row| running.contains(&row.id))
            .collect();
        if rows.is_empty() {
            window.push_notification(Notification::warning(t!("no-running-endpoints")), cx);
            return;
        }
        self.spawn_probe(rows, ping, true, Some(window.window_handle()), cx);
    }

    /// K5: Retry from a group sweep summary. The group's running set is read
    /// again so a row started or stopped since the first pass is respected.
    pub(crate) fn retry_group_probe(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_test_in_flight {
            window.push_notification(Notification::info(t!("group-test-running")), cx);
            return;
        }
        let rows = self.group_running_rows(group);
        if rows.is_empty() {
            window.push_notification(
                Notification::info(t!("group-no-running-endpoints", group = group)),
                cx,
            );
            return;
        }
        self.spawn_group_probe(rows, group.to_string(), Some(window.window_handle()), cx);
    }

    pub(crate) fn spawn_probe(
        &mut self,
        rows: Vec<ProxyConfig>,
        ping: bool,
        toast: bool,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) -> usize {
        if rows.is_empty() || self.probe_kind_in_flight(ping) {
            return 0;
        }
        let count = rows.len();
        let url = self.effective_test_url(&rows);
        let names: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.id.clone(), row.name.clone()))
            .collect();
        if ping {
            self.probe_ping_in_flight = true;
        } else {
            self.probe_url_in_flight = true;
        }
        // The batch reads the shared stop flag, so it starts from a clean one:
        // a cancelled sweep must not cancel the next run.
        self.probe_stop
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let stop = self.probe_stop.clone();
        // K5: batched runs say once that they started. Single-row runs keep
        // the established one-shot result toast, so the editor's own Health
        // tab stays the per-row detail surface.
        let progress = Arc::new(std::sync::Mutex::new(ProbeProgress {
            done: 0,
            total: count,
            failed: 0,
        }));
        let active = Arc::new(AtomicBool::new(true));
        let probe_key = self.probe_toast_key;
        let probe_toast = if toast && count > 1 {
            window.and_then(|handle| {
                handle
                    .update(cx, |_, window, cx| {
                        push_probe_start_toast(
                            probe_key,
                            probe_start_toast(count),
                            0,
                            count,
                            window,
                            cx,
                        )
                    })
                    .ok()
            })
        } else {
            None
        };
        if window.is_some() {
            spawn_probe_progress_poller(probe_toast, progress.clone(), active.clone(), cx);
        }
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let prober = std::mem::replace(&mut self.prober, Box::new(RealHealthProber::default()));
        let weak = cx.entity().downgrade();

        let toast_ping = ping;
        cx.spawn(async move |_entity, cx| {
            let sweep_progress = progress;
            let sweep_active = active;
            let (outcomes, prober) = cx
                .background_executor()
                .spawn(async move {
                    let prober = prober;
                    let counting = CountingProber {
                        inner: &*prober,
                        progress: sweep_progress,
                    };
                    let outcomes = if ping {
                        ping_all(&rows, &counting)
                    } else {
                        test_all_stoppable(&rows, &url, &counting, &stop)
                    };
                    sweep_active.store(false, std::sync::atomic::Ordering::SeqCst);
                    (outcomes, prober)
                })
                .await;
            cx.update(|cx| {
                let recorded = weak.update(cx, |view, _| {
                    view.prober = prober;
                    if ping {
                        view.probe_ping_in_flight = false;
                    } else {
                        view.probe_url_in_flight = false;
                    }
                    view.record_and_persist(&outcomes);
                    (view.health_counts(), outcomes.len())
                });
                let Ok(((live, dead), count)) = recorded else {
                    return;
                };
                let Some(view) = weak.upgrade() else {
                    return;
                };

                if toast {
                    let failed_ids: Vec<String> = outcomes
                        .iter()
                        .filter(|outcome| outcome.sample.latency_ms.is_none())
                        .map(|outcome| outcome.sample.config_id.clone())
                        .collect();
                    let message = if count == 1 {
                        match outcomes.first() {
                            Some(outcome) => {
                                let name = names
                                    .iter()
                                    .find(|(id, _)| *id == outcome.sample.config_id)
                                    .map(|(_, name)| name.clone())
                                    .unwrap_or_else(|| outcome.sample.config_id.clone());
                                probe_result_toast(&name, outcome, toast_ping)
                            }
                            None => probe_batched_toast(count, live, dead),
                        }
                    } else {
                        probe_batched_toast(count, live, dead)
                    };
                    let mut spec = if failed_ids.is_empty() {
                        ToastSpec::success(message)
                    } else {
                        ToastSpec::warning(message)
                    };
                    if !failed_ids.is_empty() {
                        let retry_ids = if count == 1 { ids.clone() } else { failed_ids };
                        let retry_view = view.downgrade();
                        spec = spec.action(
                            "toast-retry-test",
                            t!("toast-action-retry"),
                            move |window, cx| {
                                let retry_ids = retry_ids.clone();
                                let _ = retry_view.update(cx, |view, cx| {
                                    view.retry_probe(retry_ids, toast_ping, window, cx);
                                });
                            },
                        );
                    }
                    if let Some(handle) = window {
                        if handle
                            .update(cx, |_, window, cx| {
                                view.update(cx, |view, cx| {
                                    view.reload(window, cx);
                                    // K5: the start toast leaves exactly when
                                    // the result arrives.
                                    if let Some(toast) = probe_toast {
                                        close_probe_start_toast(toast, window, cx);
                                    }
                                    view.push_toast(spec, window, cx);
                                });
                            })
                            .is_ok()
                        {
                            return;
                        }
                    }
                }
                view.update(cx, |view, cx| view.reload_quiet(cx));
            });
        })
        .detach();
        count
    }

    pub(crate) fn log_lines(&self, cx: &App) -> Vec<String> {
        self.visible_log_lines(cx)
    }

    pub(crate) fn log_line_passes_filters(
        line: &str,
        exclude_lower: &str,
        pattern: Option<&regex::Regex>,
    ) -> bool {
        if !exclude_lower.is_empty() && line.to_lowercase().contains(exclude_lower) {
            return false;
        }
        pattern.is_none_or(|re| re.is_match(line))
    }

    pub(crate) fn visible_log_lines(&self, cx: &App) -> Vec<String> {
        let selected = self.selected_config(cx).map(|config| config.id);
        let filter = self.logs.filter.trim().to_lowercase();
        let level = self.logs.level;
        let render = |level: &str, message: &str| {
            format!("[{}] {}", level, redact_secrets(&strip_ansi_codes(message)))
        };
        let records: Vec<&crate::health::LogRecord> = match selected.as_deref() {
            Some(id) => self.logs.store.for_config(id),
            None => self.logs.store.all().iter().collect(),
        };
        let mut lines: Vec<String> = records
            .iter()
            .filter(|record| level.is_none_or(|want| record.level == want))
            .map(|record| render(record.level.as_str(), &record.message))
            .collect();
        if !filter.is_empty() {
            lines.retain(|line| line.to_lowercase().contains(&filter));
        }

        let exclude = self.logs.exclude.trim().to_lowercase();
        let raw_pattern = self.logs.regex.trim();
        let pattern = if raw_pattern.is_empty() {
            None
        } else {
            regex::Regex::new(raw_pattern).ok()
        };
        if !exclude.is_empty() || pattern.is_some() {
            lines.retain(|line| Self::log_line_passes_filters(line, &exclude, pattern.as_ref()));
        }
        if lines.len() > 200 {
            if self.logs.tail {
                lines = lines.split_off(lines.len() - 200);
            } else {
                lines.truncate(200);
            }
        }
        lines
    }

    pub(crate) fn set_log_level(
        &mut self,
        level: Option<LogLevel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.logs.level = level;
        if self.logs.open && window.has_active_sheet(cx) {
            self.logs.reopen_draft = self.log_input_draft(cx);
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs.open = false;
            self.open_logs_dialog(window, cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn clear_log_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.logs.filter.clear();

        self.logs.exclude.clear();
        self.logs.regex.clear();
        self.logs.level = None;
        // Drop any in-progress draft so the reopen seeds empty, not the text
        // the user was about to discard.
        self.logs.reopen_draft = None;
        self.logs.inputs = None;
        if self.logs.open && window.has_active_sheet(cx) {
            window.close_sheet(cx);
            reclaim_shell_focus(window, cx);
            self.logs.open = false;
            self.open_logs_dialog(window, cx);
        } else {
            cx.notify();
        }
    }

    /// Live text of the open sheet's three inputs (typed but not yet applied).
    pub(crate) fn log_input_draft(&self, cx: &App) -> Option<(String, String, String)> {
        self.logs.inputs.as_ref().map(|(filter, exclude, pattern)| {
            (
                filter.read(cx).value().to_string(),
                exclude.read(cx).value().to_string(),
                pattern.read(cx).value().to_string(),
            )
        })
    }

    pub(crate) fn set_row_column(
        &mut self,
        column: &'static str,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut columns = self.row_columns;
        match column {
            "upstream" => columns.upstream = enabled,
            "health" => columns.health = enabled,
            "group" => columns.group = enabled,
            _ => columns.traffic = enabled,
        }
        self.row_columns = columns;
        self.libraries.update(cx, |state, list_cx| {
            state.delegate_mut().set_row_columns(columns);
            list_cx.notify();
        });
        if let Err(error) = self.store.set_library_columns(&columns.display()) {
            window.push_notification(
                Notification::error(t!("row-columns-save-failed", error = error.to_string())),
                cx,
            );
        }
        cx.notify();
    }

    pub(crate) fn browse_for_import_files(
        &mut self,
        current_text: String,
        current_group: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        let view = cx.entity();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                _ => return,
            };
            let mut files: Vec<(String, String)> = Vec::new();
            let mut unreadable: Vec<String> = Vec::new();

            for path in &paths {
                let display = path.to_string_lossy().into_owned();
                match read_import_file(path) {
                    ImportFileRead::Text(content) => {
                        files.push((display, content));
                    }
                    ImportFileRead::Unreadable => {
                        unreadable.push(file_basename(&display));
                    }
                    ImportFileRead::Refused(reason) => {
                        unreadable.push(format!("{} ({reason})", file_basename(&display)));
                    }
                }
            }
            if files.is_empty() && unreadable.is_empty() {
                return;
            }
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_browse_pick(
                        current_text,
                        current_group,
                        files,
                        unreadable,
                        window,
                        cx,
                    );
                })
            });
        })
        .detach();
    }

    pub(crate) fn browse_for_ssh_key(
        &mut self,
        path_entity: Entity<InputState>,
        key_entity: Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let view = cx.entity();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                _ => return,
            };
            let path = paths.into_iter().next().expect("non-empty checked");
            let display = path.to_string_lossy().into_owned();
            let content = std::fs::read_to_string(&path).ok();
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_ssh_key_pick(path_entity, key_entity, display, content, window, cx);
                })
            });
        })
        .detach();
    }

    pub(crate) fn chain_add_hop(
        &mut self,
        hops: &Rc<RefCell<Vec<serde_json::Value>>>,
        library: &Rc<RefCell<Vec<ChainLibraryEntry>>>,
        pick: &Entity<SelectState<PanelChoiceList>>,
        picked_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let picked_id = picked_id.trim();
        let entry = library
            .borrow()
            .iter()
            .find(|entry| entry.id == picked_id)
            .cloned();
        let Some(entry) = entry else {
            window.push_notification(Notification::warning(t!("pick-library-row-first")), cx);
            return false;
        };
        let kinds = chain_existing_kinds(&hops.borrow(), &library.borrow());
        if let Some(rule) =
            chain_hop_rejection(&kinds, chain_slot_kind(&entry.protocol), kinds.len(), false)
        {
            window.push_notification(Notification::warning(chain_hop_rejection_message(rule)), cx);
            return false;
        }
        hops.borrow_mut()
            .push(serde_json::Value::String(entry.id.clone()));
        pick.update(cx, |state, cx| {
            state.set_selected_value(&String::new(), window, cx);
        });
        window.push_notification(
            Notification::success(t!("chain-hop-added", name = entry.name)),
            cx,
        );
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn chain_replace_hop(
        &mut self,
        hops: &Rc<RefCell<Vec<serde_json::Value>>>,
        library: &Rc<RefCell<Vec<ChainLibraryEntry>>>,
        pick: &Entity<SelectState<PanelChoiceList>>,
        index: usize,
        picked_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let picked_id = picked_id.trim();
        let entry = library
            .borrow()
            .iter()
            .find(|entry| entry.id == picked_id)
            .cloned();
        let Some(entry) = entry else {
            window.push_notification(Notification::warning(t!("pick-library-row-first")), cx);
            return false;
        };
        let kinds = chain_existing_kinds(&hops.borrow(), &library.borrow());
        if let Some(rule) =
            chain_hop_rejection(&kinds, chain_slot_kind(&entry.protocol), index, true)
        {
            window.push_notification(Notification::warning(chain_hop_rejection_message(rule)), cx);
            return false;
        }
        hops.borrow_mut()[index] = serde_json::Value::String(entry.id.clone());
        pick.update(cx, |state, cx| {
            state.set_selected_value(&String::new(), window, cx);
        });
        window.push_notification(
            Notification::success(t!(
                "chain-hop-replaced",
                index = index + 1,
                name = entry.name
            )),
            cx,
        );
        true
    }

    pub(crate) fn browse_for_extra_exe(
        &mut self,
        path_entity: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let view = cx.entity();
        let handle = window.window_handle();
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) if !paths.is_empty() => paths,
                _ => return,
            };
            let path = paths.into_iter().next().expect("non-empty checked");
            let display = path.to_string_lossy().into_owned();
            let _ = handle.update(cx, |_, window, cx| {
                view.update(cx, |view, cx| {
                    view.finish_extra_exe_pick(path_entity, display, window, cx);
                })
            });
        })
        .detach();
    }

    /// Keeps the dialog footer's mirror in step with the parse state.
    pub(crate) fn set_import_parse_in_flight(&mut self, in_flight: bool) {
        self.import_parse_in_flight = in_flight;
        if let Some(form) = &self.import_form {
            form.in_flight.set(in_flight);
        }
    }

    /// Marks the running (or just-finished) import parse as cancelled so its
    /// batch is dropped instead of landing after the dialog was dismissed.
    pub(crate) fn cancel_import_parse(&mut self, cx: &mut Context<Self>) {
        self.import_cancel_epoch = Some(self.import_parse_epoch);
        cx.notify();
    }

    pub(crate) fn try_addsub_commit(
        &mut self,
        resolved: &ResolvedImport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        let trimmed = resolved.text.trim();
        if trimmed.is_empty() || trimmed.contains('\n') || resolved.file_hint.is_some() {
            return None;
        }
        if !is_addsub_attempt(trimmed) {
            return None;
        }
        Some(match parse_addsub_link(trimmed) {
            Ok(request) => {
                let effective = effective_addsub_group(&resolved.group, &request.group);
                self.commit_addsub_request_with_group(request, &effective, window, cx)
            }
            Err(error) => {
                window.push_notification(Notification::error(addsub_error_message(&error)), cx);
                false
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn accumulate_batch_member(
        &mut self,
        sub_id: &str,
        added: usize,
        updated: usize,
        removed: usize,
        failed: usize,
        conflicts: usize,
        in_use: usize,
        added_names: &[String],
        updated_names: &[String],
        removed_names: &[String],
        kept_in_use_names: &[String],
    ) -> bool {
        if !self.refresh.members.remove(sub_id) {
            return false;
        }
        self.refresh.added += added;
        self.refresh.updated += updated;
        self.refresh.removed += removed;
        self.refresh.failed += failed;
        self.refresh.conflicts += conflicts;
        self.refresh.in_use += in_use;
        self.refresh.added_names.extend(added_names.iter().cloned());
        self.refresh
            .updated_names
            .extend(updated_names.iter().cloned());
        self.refresh
            .removed_names
            .extend(removed_names.iter().cloned());
        self.refresh
            .in_use_names
            .extend(kept_in_use_names.iter().cloned());
        self.refresh.members.is_empty()
    }

    pub(crate) fn is_batch_member(&self, sub_id: &str) -> bool {
        self.refresh.members.contains(sub_id)
    }

    pub(crate) fn refresh_batch_progress(&self) -> Option<(usize, usize)> {
        if self.refresh.members.is_empty() {
            return None;
        }
        let total = self.refresh.total.max(self.refresh.members.len());
        Some((total.saturating_sub(self.refresh.members.len()), total))
    }

    pub(crate) fn close_batch_slot(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_batch_member(sub_id)
            && self.accumulate_batch_member(sub_id, 0, 0, 0, 0, 0, 0, &[], &[], &[], &[])
        {
            self.flush_refresh_batch(window, cx);
        }
    }

    pub(crate) fn push_refresh_log(&mut self, line: &str) {
        self.logs.store.push_raw(line, &LogTagIndex::default());
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn route_refresh_completion(
        &mut self,
        sub_id: &str,
        notice: ToastSpec,
        added: usize,
        updated: usize,
        removed: usize,
        failed: usize,
        conflicts: usize,
        in_use: usize,
        added_names: &[String],
        updated_names: &[String],
        removed_names: &[String],
        kept_in_use_names: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_batch_member(sub_id) {
            if self.accumulate_batch_member(
                sub_id,
                added,
                updated,
                removed,
                failed,
                conflicts,
                in_use,
                added_names,
                updated_names,
                removed_names,
                kept_in_use_names,
            ) {
                self.flush_refresh_batch(window, cx);
            }
        } else {
            // A failed refresh leaves the user looking at the library; the
            // next step is usually the Subscriptions sheet, so put it on the
            // toast instead of making them hunt for the menu row.
            let notice = if failed > 0 {
                self.os_notify(
                    crate::os::notify::NoticeKind::SubscriptionFailure,
                    &t!("subscription-refresh-failed-title"),
                    &t!("subscription-refresh-failed-body"),
                );
                let view = cx.entity().downgrade();
                notice.action(
                    "toast-show-subscriptions",
                    t!("toast-action-show"),
                    move |window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.open_subscriptions_dialog(window, cx);
                        });
                    },
                )
            } else {
                notice
            };
            self.push_toast(notice, window, cx);
        }
    }

    pub(crate) fn refresh_subscription_by_id(
        &mut self,
        sub_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.refresh.members.remove(sub_id) {
            self.refresh.total = self.refresh.total.saturating_sub(1);
        }
        let started = match self.store.get_subscription(sub_id) {
            Ok(Some(sub)) => self.start_subscription_refresh(sub, true, cx),
            Ok(None) => {
                window.push_notification(Notification::warning(t!("already-removed")), cx);
                false
            }
            Err(error) => {
                let view = cx.entity().downgrade();
                self.push_toast(
                    ToastSpec::error(t!("cannot-refresh-subscription", error = error.to_string()))
                        .action(
                            "toast-show-subscriptions",
                            t!("toast-action-show"),
                            move |window, cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.open_subscriptions_dialog(window, cx);
                                });
                            },
                        ),
                    window,
                    cx,
                );
                false
            }
        };

        if started {
            self.reopen_subscriptions_if_open(window, cx);
        }
        started
    }

    pub(crate) fn note_refresh_started(&mut self, sub_id: &str) -> bool {
        self.refresh.inflight.insert(sub_id.to_string())
    }

    /// B16: every refresh origin funnels through `start_subscription_refresh`,
    /// so the list learns it is waiting from one place and never has to read
    /// the app's in-flight set while a frame is borrowing it.
    pub(crate) fn mark_library_loading(&mut self, cx: &mut App) {
        let load = LibraryLoad::Loading(t!("library-loading-subscriptions"));
        self.libraries.update(cx, |state, cx| {
            if state.delegate_mut().set_load(load) {
                cx.notify();
            }
        });
    }

    /// B16: settle once nothing is in flight. A failure with no rows left is
    /// the list's state to report; with rows on screen the toast already said
    /// it and the list stays out of the way.
    pub(crate) fn settle_library_load(&mut self, cx: &mut App) {
        if !self.refresh.inflight.is_empty() {
            return;
        }
        let failed = self.refresh.last_error.take();
        self.libraries.update(cx, |state, cx| {
            let load = match failed {
                Some(message) if state.delegate().rows.is_empty() => LibraryLoad::Failed(message),
                _ => LibraryLoad::Ready,
            };
            if state.delegate_mut().set_load(load) {
                cx.notify();
            }
        });
    }

    /// B16: the failed state's way back. Retry means "fetch my subscriptions
    /// again", which is the only thing that can fill an empty list.
    pub(crate) fn retry_library_load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let subs = self.store.list_subscriptions().unwrap_or_default();
        if subs.is_empty() {
            self.settle_library_load(cx);
            return;
        }
        self.refresh.last_error = None;
        self.start_explicit_refresh(subs, window, cx);
    }

    pub(crate) fn sync_presets_to_engine(&mut self) -> Result<(), String> {
        match self.store.preset_rules_map() {
            Ok(map) => {
                self.engine.set_preset_rules(map);
            }
            Err(error) => {
                return Err(t!(
                    "cannot-load-routing-presets-lower",
                    error = error.to_string()
                ))
            }
        }
        self.sync_rule_sets_to_engine()
    }

    pub(crate) fn try_preset_commit(
        &mut self,
        resolved: &ResolvedImport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        use crate::preset::{
            decode_remote_route_link, is_remote_route_attempt, is_route_attempt,
            looks_like_preset_json,
        };
        let trimmed = resolved.text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let single_line =
            !trimmed.contains('\n') && resolved.file_hint.is_none() && resolved.multi.is_none();
        if single_line && is_route_attempt(trimmed) {
            return Some(match self.store.import_route_link(trimmed) {
                Ok(preset) => {
                    let count = preset.rules().map(|rules| rules.len()).unwrap_or(0);
                    self.reload(window, cx);
                    window.push_notification(
                        Notification::success(preset_imported_message(&preset.name, count)),
                        cx,
                    );
                    true
                }
                Err(error) => {
                    self.report_save_error(error, window, cx);
                    false
                }
            });
        }
        if single_line && is_remote_route_attempt(trimmed) {
            return Some(match decode_remote_route_link(trimmed) {
                Ok(request) => {
                    match self
                        .store
                        .import_remote_preset_shell(&request.name, &request.url)
                    {
                        Ok(preset) => {
                            self.reload(window, cx);
                            window.push_notification(
                                Notification::success(t!(
                                    "routing-preset-added-fetching",
                                    name = preset.name.as_str()
                                )),
                                cx,
                            );
                            self.start_preset_refresh(preset, cx);
                            true
                        }
                        Err(error) => {
                            self.report_save_error(error, window, cx);
                            false
                        }
                    }
                }
                Err(error) => {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    false
                }
            });
        }
        if resolved.multi.is_none() && looks_like_preset_json(trimmed) {
            return Some(
                match self.store.import_preset_json(
                    trimmed,
                    if resolved.file_hint.is_some() {
                        "file"
                    } else {
                        "manual"
                    },
                    resolved.file_hint.as_deref(),
                ) {
                    Ok(preset) => {
                        let count = preset.rules().map(|rules| rules.len()).unwrap_or(0);
                        self.reload(window, cx);
                        window.push_notification(
                            Notification::success(preset_imported_message(&preset.name, count)),
                            cx,
                        );
                        true
                    }
                    Err(error) => {
                        self.report_save_error(error, window, cx);
                        false
                    }
                },
            );
        }
        None
    }

    /// T-11: expand the simple-rules boxes into structured rules, appending
    /// them to the hand-built rules. Any invalid line blocks the save loud:
    /// nothing is dropped, corrected, or stored, and the boxes keep their
    /// text for the editor's "Invalid lines" fold. The expanded rules flow
    /// through the store's own `validate_preset_rules` gate unchanged — no
    /// new intake errors here.
    pub(crate) fn expand_simple_rule_boxes(
        boxes: &SimpleRuleBoxes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Vec<serde_json::Value>, ()> {
        let mut rules: Vec<serde_json::Value> = Vec::new();
        let mut invalid: Vec<String> = Vec::new();
        for (r#box, text) in [
            (crate::preset::SimpleRulesBox::Direct, boxes.direct.as_str()),
            (crate::preset::SimpleRulesBox::Block, boxes.block.as_str()),
            (crate::preset::SimpleRulesBox::Proxy, boxes.proxy.as_str()),
        ] {
            if text.trim().is_empty() {
                continue;
            }
            let parsed = crate::preset::parse_simple_rules(r#box, r#box.action(), text);
            invalid.extend(parsed.invalid.into_iter().map(|line| {
                t!(
                    "simple-box-invalid-line",
                    box_name = simple_rules_box_label(r#box),
                    line = line
                )
            }));
            rules.extend(parsed.rules);
        }
        if invalid.is_empty() {
            return Ok(rules);
        }
        window.push_notification(
            Notification::error(t!("invalid-simple-rules", lines = invalid.join(" | "))),
            cx,
        );
        Err(())
    }

    pub(crate) fn rewrite_preset_rules_box(
        form: &PresetForm,
        op: &PresetRulesOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = form.name.read(cx).value().to_string();
        let text = form.rules.read(cx).value().to_string();
        let fallback = if name.trim().is_empty() {
            "preset"
        } else {
            name.trim()
        };
        let mut parsed = match crate::preset::parse_preset_json(&text, fallback) {
            Ok(parsed) => parsed.rules,
            Err(error) => {
                window.push_notification(Notification::error(preset_error_message(&error)), cx);
                return;
            }
        };
        match op {
            PresetRulesOp::CycleAction(index) => {
                let Some(rule) = parsed.get_mut(*index) else {
                    window.push_notification(Notification::warning(t!("already-removed")), cx);
                    return;
                };
                let current = rule
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("direct");
                let next = match current {
                    "direct" => "proxy",
                    "proxy" => "block",
                    _ => "direct",
                };
                if let Err(error) = crate::preset::set_rule_action(rule, next) {
                    window.push_notification(Notification::error(preset_error_message(&error)), cx);
                    return;
                }
            }
            PresetRulesOp::Remove(index) => {
                if *index >= parsed.len() {
                    window.push_notification(Notification::warning(t!("already-removed")), cx);
                    return;
                }
                parsed.remove(*index);
            }
            PresetRulesOp::Add => {
                parsed.push(serde_json::json!({"action": "direct"}));
            }
        }
        let next_text = serde_json::Value::Array(parsed).to_string();
        form.rules.update(cx, |state, cx| {
            state.set_value(next_text, window, cx);
        });
    }

    pub(crate) fn preset_name_for(&self, preset_id: Option<&str>) -> Option<String> {
        let id = preset_id?;
        self.store
            .get_preset(id)
            .ok()
            .flatten()
            .map(|preset| preset.name)
    }

    pub(crate) fn refresh_preset_by_id(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.store.get_preset(id) {
            Ok(Some(preset)) => {
                if preset.source != "url" || preset.source_url.is_none() {
                    window.push_notification(
                        Notification::warning(t!("only-url-presets-refresh")),
                        cx,
                    );
                    return false;
                }
                self.start_preset_refresh(preset, cx);
                true
            }
            Ok(None) => {
                window.push_notification(Notification::warning(t!("already-removed")), cx);
                false
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!(
                        "cannot-refresh-routing-preset",
                        error = error.to_string()
                    )),
                    cx,
                );
                false
            }
        }
    }

    // ——— T-12: .srs rule-set staging (explicit confirm-per-download only) ——

    pub(crate) fn sync_rule_sets_to_engine(&mut self) -> Result<(), String> {
        let dir = crate::store::rule_sets_dir();
        let files: HashMap<String, PathBuf> = self
            .store
            .list_rule_sets()
            .map_err(|error| t!("cannot-load-rule-sets-lower", error = error.to_string()))?
            .into_iter()
            .map(|row| (row.name, dir.join(row.file_name)))
            .collect();
        self.engine.set_rule_set_files(files);
        Ok(())
    }

    pub(crate) fn refresh_due_on_startup(&mut self, cx: &mut Context<Self>) {
        for sub in self.due_subscriptions() {
            self.start_subscription_refresh(sub, false, cx);
        }
    }

    pub(crate) fn refresh_due_on_startup_deferred(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(STARTUP_REFRESH_DEFER_SECS))
                .await;
            cx.update(|cx| {
                weak.update(cx, |view, cx| {
                    if !view.ticks_enabled {
                        return;
                    }
                    view.refresh_due_on_startup(cx);
                })
                .ok();
            });
        })
        .detach();
    }

    pub(crate) fn due_subscriptions(&self) -> Vec<ProxySubscription> {
        if !self.store.subs_auto_refresh_enabled() {
            return Vec::new();
        }

        self.due_for_manual_refresh()
            .into_iter()
            .filter(|sub| !self.store.group_skip_refresh(&sub.group))
            .collect()
    }

    pub(crate) fn due_for_manual_refresh(&self) -> Vec<ProxySubscription> {
        let now = now_unix();
        self.store
            .list_subscriptions()
            .unwrap_or_default()
            .into_iter()
            .filter(|sub| due_for_refresh_for(sub, now))
            .collect()
    }

    pub(crate) fn refresh_all_due(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let due = self.due_for_manual_refresh();
        if due.is_empty() {
            window.push_notification(Notification::success(t!("no-subs-due")), cx);
            return 0;
        }
        let ids: Vec<String> = due.iter().map(|sub| sub.id.clone()).collect();
        self.begin_refresh_batch(&ids);
        let mut started = 0;
        for sub in due {
            if self.start_subscription_refresh(sub, true, cx) {
                started += 1;
            }
        }
        self.reopen_subscriptions_if_open(window, cx);
        started
    }

    pub(crate) fn current_group(&self, cx: &App) -> Option<String> {
        if let Some(group) = self.libraries.read(cx).delegate().facet_group.clone() {
            return Some(group);
        }
        self.selected_config(cx).map(|config| config.group)
    }

    pub(crate) fn update_current_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.current_group(cx) else {
            window.push_notification(Notification::info(t!("no-group-focus")), cx);
            return;
        };
        let subs = self
            .store
            .list_subscriptions()
            .map(|subs| update_target_subs(&subs, &group))
            .unwrap_or_default();
        if subs.is_empty() {
            window.push_notification(Notification::info(t!("group-no-subs", group = group)), cx);
            return;
        }
        self.start_explicit_refresh(subs, window, cx);
    }

    pub(crate) fn update_group_subs(
        &mut self,
        group: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let subs = self
            .store
            .list_subscriptions()
            .map(|subs| update_target_subs(&subs, group))
            .unwrap_or_default();
        if subs.is_empty() {
            window.push_notification(Notification::info(t!("group-no-subs", group = group)), cx);
            return;
        }
        self.start_explicit_refresh(subs, window, cx);
    }

    /// Probe endpoints whose own bytes must stay out of row usage: the default
    /// test URL, the global custom test URL, and every running row's override.
    pub(crate) fn traffic_probe_exclusions(&self) -> ProbeExclusions {
        let mut exclusions = ProbeExclusions::new();
        exclusions.push_url(DEFAULT_TEST_URL);
        if let Some(global) = self.store.test_url() {
            exclusions.push_url(&global);
        }
        for id in self.engine.running_ids() {
            if let Some(url) = self.store.row_test_url(&id) {
                exclusions.push_url(&url);
            }
        }
        exclusions
    }

    pub(crate) fn spawn_traffic_poll(&mut self, cx: &mut Context<Self>) -> usize {
        if !self.ticks_enabled || self.traffic_tick_in_flight {
            return 0;
        }

        let now = now_unix();
        if !traffic_poll_due(self.inspector_tab, now, self.conn.last_poll_at) {
            return 0;
        }
        self.conn.last_poll_at = now;
        let targets = self.engine.live_clash_targets();
        if targets.is_empty() {
            return 0;
        }
        self.ensure_traffic_mirror(cx);
        self.traffic_tick_in_flight = true;
        let exclusions = self.traffic_probe_exclusions();
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_entity, cx| {
            let (results, exclusions) = cx
                .background_executor()
                .spawn(async move {
                    let results = crate::health::poll_clash_targets(
                        &targets,
                        crate::health::CLASH_PER_CORE_TIMEOUT,
                        crate::health::CLASH_TOTAL_DEADLINE,
                    );
                    let mut exclusions = exclusions;
                    exclusions.resolve_ips();
                    (results, exclusions)
                })
                .await;
            cx.update(|cx| {
                weak.update(cx, |view, cx| {
                    view.traffic_tick_in_flight = false;
                    view.apply_traffic_results(results, &exclusions, cx);
                })
                .ok();
            });
        })
        .detach();
        1
    }

    pub(crate) fn usage_line(&self, config_id: &str) -> String {
        match self.store.traffic_totals() {
            Ok(totals) => match totals.get(config_id) {
                Some((rx, tx)) => t!(
                    "usage-line",
                    down = format_bytes(*rx),
                    up = format_bytes(*tx)
                ),
                None => t!("stats-usage-unavailable"),
            },
            Err(_) => t!("stats-usage-unavailable"),
        }
    }

    pub(crate) fn usage_parts(&self, config_id: &str) -> Option<(String, String)> {
        match self.store.traffic_totals() {
            Ok(totals) => totals
                .get(config_id)
                .map(|(rx, tx)| (format_bytes(*rx), format_bytes(*tx))),
            Err(_) => None,
        }
    }

    pub(crate) fn history_line(&self, config_id: &str, cx: &App) -> AnyElement {
        let since = now_unix().saturating_sub(90 * 86_400);
        let entries = match self.store.load_health_history(config_id, Some(since)) {
            Ok(entries) => entries,
            Err(_) => {
                return div()
                    .child(t!("detail-history-unavailable"))
                    .into_any_element()
            }
        };
        let latencies = url_test_sparkline(&entries, 120);
        let caption_tone = sparkline_caption_tone(&entries);
        if latencies.len() < 2 {
            return div()
                .id("config-history-empty")
                .test_support()
                .w_full()
                .min_w_0()
                .text_color(health_tone_color(caption_tone, cx.theme()))
                .child(match latencies.len() {
                    1 => t!("detail-history-one", latency = latencies[0]),
                    _ => t!("detail-history-none"),
                })
                .into_any_element();
        }
        let latest = latencies.last().copied().unwrap_or(0);

        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .text_color(health_tone_color(caption_tone, cx.theme()))
                    .child(t!(
                        "detail-history-samples",
                        count = latencies.len(),
                        latest = latest
                    )),
            )
            .child(Self::latency_sparkline(&latencies))
            .into_any_element()
    }

    // B18: the whole motion vocabulary lives below this line — one duration,
    // one easing, two primitives. Nothing loops, nothing exceeds the budget,
    // and reduced motion turns all of it off.

    pub(crate) const MOTION_FAST_MS: u64 = 150;

    /// How long a `Copied!` transient marker stays painted after a cell click.
    pub(crate) const CONN_COPIED_TOOLTIP_MS: u64 = 1500;

    pub(crate) fn motion_fast_duration(cx: &App) -> Duration {
        if cx.reduce_motion() {
            Duration::ZERO
        } else {
            Duration::from_millis(Self::MOTION_FAST_MS)
        }
    }

    /// Quick off the mark, soft landing. No overshoot: nothing here bounces.
    pub(crate) fn motion_ease_out() -> impl Fn(f32) -> f32 + 'static {
        ease_out_quint()
    }

    /// The entry fade, applied to an element where it stands. The element keeps
    /// its own layout, so an absolutely positioned surface (the inspector
    /// drawer) does not reflow the list behind it and a padded bar keeps its
    /// padding.
    pub(crate) fn motion_fade<E>(element: E, id: impl Into<ElementId>, cx: &App) -> impl IntoElement
    where
        E: Styled + IntoElement + 'static,
    {
        element.with_animation(
            id,
            Animation::new(Self::motion_fast_duration(cx)).with_easing(Self::motion_ease_out()),
            |this, delta| this.opacity(delta),
        )
    }

    pub(crate) fn motion_enter(
        child: impl IntoElement + 'static,
        id: impl Into<ElementId>,
        cx: &App,
    ) -> impl IntoElement {
        Self::motion_fade(div().child(child), id, cx)
    }

    /// The accent spends half the budget going up and half coming back down —
    /// one blink, never a loop, and never longer than the entry fade.
    pub(crate) const fn motion_accent_halves_ms() -> u64 {
        Self::MOTION_FAST_MS / 2
    }

    /// The accent that marks the one row whose engine state just settled.
    /// Reduced motion returns `None`, so the caller paints neither the strip
    /// nor its animation state.
    pub(crate) fn motion_accent<E>(
        accent: E,
        id: impl Into<ElementId>,
        cx: &App,
    ) -> Option<impl IntoElement>
    where
        E: Styled + IntoElement + 'static,
    {
        let duration = Self::motion_fast_duration(cx);
        if duration.is_zero() {
            return None;
        }
        let half = Duration::from_millis(Self::motion_accent_halves_ms());
        Some(accent.with_animations(
            id,
            vec![
                Animation::new(half).with_easing(Self::motion_ease_out()),
                Animation::new(half),
            ],
            |this, phase, delta| this.opacity(if phase == 0 { delta } else { 1. - delta }),
        ))
    }

    /// F19: picking a tab is a way in — the shortcut and the tab strip both
    /// land here, so a hidden inspector can never keep a chosen tab to itself.
    pub(crate) fn set_inspector_tab(&mut self, tab: InspectorTab) {
        self.inspector_visible = true;
        self.inspector_tab = tab;
        self.detail_scroll.set_offset(point(px(0.), px(0.)));
    }

    /// F19: Ctrl-1 is the one key that owns presence, so it toggles the panel
    /// shut instead of only ever opening it. Another tab key still opens.
    pub(crate) fn toggle_inspector(&mut self, tab: InspectorTab) {
        let already = self.inspector_visible && self.inspector_tab == tab;
        if already {
            self.inspector_visible = false;
            return;
        }
        self.set_inspector_tab(tab);
    }

    pub(crate) fn adjust_conn_column_widths(
        &mut self,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let widths = self.store.conn_column_widths();
        for (index, width) in widths.iter().enumerate() {
            let next = (*width as i32 + delta).clamp(40, 600) as u32;
            if let Err(error) = self.store.set_conn_column_width(index, next) {
                window.push_notification(
                    Notification::error(t!("column-width-failed", error = error.to_string())),
                    cx,
                );
                return;
            }
        }
        cx.notify();
    }

    pub(crate) fn refresh_conn_column_widths(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.reset_conn_column_widths() {
            Ok(()) => {
                window.push_notification(Notification::success(t!("column-widths-reset")), cx);
                cx.notify();
            }
            Err(error) => {
                window.push_notification(
                    Notification::error(t!("column-width-reset-failed", error = error.to_string())),
                    cx,
                );
            }
        }
    }

    pub(crate) fn detail_failed(&self, config: &ProxyConfig) -> bool {
        self.engine.ids_on_crashed_shards().contains(&config.id)
            || self.health_store.row_summary(&config.id).trim() == "dead"
    }

    pub(crate) fn select_visible_checked(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.libraries.update(cx, |state, list_cx| {
            let ids: Vec<String> = state
                .delegate()
                .visible_rows()
                .iter()
                .map(|row| row.id.clone())
                .collect();
            let delegate = state.delegate_mut();
            for id in ids {
                if !delegate.checked.contains(&id) {
                    delegate.toggle_checked(&id);
                }
            }
            list_cx.notify();
        });
        cx.notify();
    }
}
