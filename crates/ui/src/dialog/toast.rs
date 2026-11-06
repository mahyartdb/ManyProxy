use crate::*;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::prelude::FluentBuilder as _;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::time::Duration;
use web_time::Instant;

/// K5: the whole toast surface speaks one grammar — park bottom-right above
/// the status bar, at most two visible, and dismiss on our own clock so a
/// success toast (~4s) beats an error toast (~8s). `autohide(false)` plus a
/// timer closed by `remove_notification1` is the only route to a non-default
/// lifetime, because the kit itself bakes 5s into `NotificationList::push`.
pub(crate) struct ToastLifetime;

/// ~4s for success toasts (spec: ~4s success).
pub(crate) const TOAST_SUCCESS_MS: u64 = 4200;
/// ~8s for error toasts (spec: ~8s error).
pub(crate) const TOAST_ERROR_MS: u64 = 8000;
/// Error toasts stop yielding to hover after this. Hover buys reading time,
/// not a permanent corner resident: a mouse parked on a toast from hours ago
/// must not hide every later toast behind it.
pub(crate) const TOAST_ERROR_HARD_CAP_MS: u64 = 15_000;
/// Delete→Undo toast lifetime; the removed rows live in memory exactly as long.
pub(crate) const TOAST_UNDO_MS: u64 = 8000;
/// The status bar is ~26px tall; a cushion above it is the small safe-area
/// offset, so a toast can never cover the bar or sit on the window edge.
const TOAST_BOTTOM_MARGIN: f32 = 36.0;
const TOAST_EDGE_MARGIN: f32 = 16.0;

/// Buttons a toast may carry. The kit's `Notification::action` renders one
/// button on the right; a second action needs a row of its own, so every
/// action is rendered by the toast body and the whole body owns the hover
/// state. Two is the ceiling named by the feedback spec.
const TOAST_MAX_ACTIONS: usize = 2;

/// K5 install of the parking rules. Runs right after every `Theme::change`
/// (boot via `apply_appearance`, plus every appearance toggle): the theme
/// change rebuilds the whole theme — including these settings — so the park
/// must be re-stamped on the fresh theme each time.
pub(crate) fn configure_notification_toasts(cx: &mut App) {
    let defaults = gpui_kit::component::notification::NotificationSettings::default();
    let notification = &mut gpui_kit::component::Theme::global_mut(cx).notification;
    notification.placement = Anchor::BottomRight;
    notification.max_items = 2;
    notification.margins = gpui_kit::component::Edges {
        top: defaults.margins.top,
        right: px(TOAST_EDGE_MARGIN),
        bottom: px(TOAST_BOTTOM_MARGIN),
        left: defaults.margins.left,
    };
}

/// K5 counter for unique element ids of timed toasts; undo batch ids share
/// it, so a batch maps one-to-one onto one removable toast.
static NEXT_TOAST_KEY: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_undo_batch_id() -> u64 {
    NEXT_TOAST_KEY.fetch_add(1, Ordering::SeqCst)
}

pub(crate) fn next_toast_key() -> u64 {
    NEXT_TOAST_KEY.fetch_add(1, Ordering::SeqCst)
}

/// Each timer scheduling owns a generation, so a replaced or paused clock's
/// pending wake-up can never close the toast that now holds the same key.
static NEXT_CLOCK_GENERATION: AtomicU64 = AtomicU64::new(1);

fn next_clock_generation() -> u64 {
    NEXT_CLOCK_GENERATION.fetch_add(1, Ordering::SeqCst)
}

struct UndoBatch {
    key: u64,
    rows: Vec<ProxyConfig>,
}

fn undo_stash() -> &'static Mutex<Vec<UndoBatch>> {
    static STASH: OnceLock<Mutex<Vec<UndoBatch>>> = OnceLock::new();
    STASH.get_or_init(|| Mutex::new(Vec::new()))
}

/// The `AppView` struct lives at the crate root and its field block is owned
/// by another wave, so the undo records live beside the toasts that carry
/// them — the toast lifetime literally keeping the rows readable is what the
/// ticket names ("stores removed rows in memory for the toast lifetime").
pub(crate) fn stash_undo_rows(rows: Vec<ProxyConfig>, key: u64) {
    undo_stash()
        .lock()
        .expect("undo stash lock")
        .push(UndoBatch { key, rows });
}

pub(crate) fn take_undo_rows(key: u64) -> Option<Vec<ProxyConfig>> {
    let mut stash = undo_stash().lock().expect("undo stash lock");
    let index = stash.iter().position(|batch| batch.key == key)?;
    let batch = stash.remove(index);
    Some(batch.rows)
}

pub(crate) fn drop_undo_batch(key: u64) {
    let mut stash = undo_stash().lock().expect("undo stash lock");
    stash.retain(|batch| batch.key != key);
}

/// Hermetic read of the cross-test stash; tests in one process share it, so
/// assertions are name-scoped instead of absolute. Never called in prod.
#[cfg(test)]
pub(crate) fn undo_stash_count_named(name: &str) -> usize {
    undo_stash()
        .lock()
        .expect("undo stash lock")
        .iter()
        .filter(|batch| batch.rows.iter().any(|row| row.name == name))
        .count()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastTone {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastTone {
    fn notification(self) -> Notification {
        Notification::new().with_type(match self {
            ToastTone::Info => NotificationType::Info,
            ToastTone::Success => NotificationType::Success,
            ToastTone::Warning => NotificationType::Warning,
            ToastTone::Error => NotificationType::Error,
        })
    }

    fn lifetime_ms(self) -> u64 {
        match self {
            ToastTone::Error => TOAST_ERROR_MS,
            _ => TOAST_SUCCESS_MS,
        }
    }

    fn hard_cap_ms(self) -> Option<u64> {
        match self {
            ToastTone::Error => Some(TOAST_ERROR_HARD_CAP_MS),
            _ => None,
        }
    }
}

type ToastRun = Rc<dyn Fn(&mut Window, &mut App)>;

pub(crate) struct ToastAction {
    id: &'static str,
    label: String,
    run: ToastRun,
}

/// Identifies one push, not just the key. A replacement under the same key
/// must not let the previous sweep close or repaint the new toast.
#[derive(Clone, Copy)]
pub(crate) struct ToastHandle {
    key: u64,
    instance: u64,
}

impl ToastHandle {
    #[cfg(test)]
    pub(crate) fn key(self) -> u64 {
        self.key
    }
}

/// One toast worth of intent, decoupled from the timer and the element tree.
/// Callers describe tone/copy/actions/progress; `push_toast_spec` owns the
/// key, the hover clock and the notification's lifetime.
pub(crate) struct ToastSpec {
    tone: ToastTone,
    message: String,
    actions: Vec<ToastAction>,
    progress: Option<(usize, usize)>,
    key: Option<u64>,
    timed: bool,
    lifetime_ms: u64,
    hard_cap_ms: Option<u64>,
    cleanup: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl ToastSpec {
    pub(crate) fn new(tone: ToastTone, message: impl Into<String>) -> Self {
        Self {
            tone,
            message: message.into(),
            actions: Vec::new(),
            progress: None,
            key: None,
            timed: true,
            lifetime_ms: tone.lifetime_ms(),
            hard_cap_ms: tone.hard_cap_ms(),
            cleanup: None,
        }
    }

    pub(crate) fn info(message: impl Into<String>) -> Self {
        Self::new(ToastTone::Info, message)
    }

    pub(crate) fn success(message: impl Into<String>) -> Self {
        Self::new(ToastTone::Success, message)
    }

    pub(crate) fn warning(message: impl Into<String>) -> Self {
        Self::new(ToastTone::Warning, message)
    }

    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self::new(ToastTone::Error, message)
    }

    /// Fixed key: a repeat push replaces the previous toast instead of
    /// stacking a second copy of the same story.
    pub(crate) fn key(mut self, key: u64) -> Self {
        self.key = Some(key);
        self
    }

    pub(crate) fn lifetime(mut self, lifetime_ms: u64) -> Self {
        self.timed = true;
        self.lifetime_ms = lifetime_ms;
        self
    }

    /// No expiry timer; the caller closes the toast when the work ends.
    pub(crate) fn no_timeout(mut self) -> Self {
        self.timed = false;
        self
    }

    pub(crate) fn progress(mut self, done: usize, total: usize) -> Self {
        self.progress = Some((done, total));
        self
    }

    pub(crate) fn action(
        mut self,
        id: &'static str,
        label: impl Into<String>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        if self.actions.len() < TOAST_MAX_ACTIONS {
            self.actions.push(ToastAction {
                id,
                label: label.into(),
                run: Rc::new(run),
            });
        }
        self
    }

    /// Runs when the toast leaves, by timer or by the user. Undo uses it to
    /// drop the stashed rows at exactly the toast's lifetime.
    pub(crate) fn cleanup(mut self, cleanup: impl FnOnce() + Send + 'static) -> Self {
        self.cleanup = Some(Box::new(cleanup));
        self
    }
}

/// A live progress ring. It lives as its own entity so the probe sweep can
/// repaint just the ring when a row finishes; re-pushing the notification
/// would replay the enter animation on every probe.
pub(crate) struct ToastProgress {
    done: usize,
    total: usize,
}

impl ToastProgress {
    fn new(done: usize, total: usize) -> Self {
        Self { done, total }
    }

    fn value(&self) -> f32 {
        if self.total == 0 {
            0.
        } else {
            (self.done.min(self.total) as f32 / self.total as f32) * 100.
        }
    }

    fn label(&self) -> String {
        probe_remaining_label(self.done, self.total)
    }
}

/// The one copy for "how much is left", shared by the ring and its tests.
pub(crate) fn probe_remaining_label(done: usize, total: usize) -> String {
    match total.saturating_sub(done) {
        1 => t!("probe-remaining-one"),
        remaining => t!("probe-remaining", count = remaining),
    }
}

impl Render for ToastProgress {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .items_center()
            .gap_2()
            .child(
                ProgressCircle::new(ElementId::View(cx.entity().entity_id()))
                    .value(self.value())
                    .small(),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.label()),
            )
    }
}

/// Expiry lives outside the toast entity: the kit bakes its own 5s autohide
/// and never exposes a hover hook on the card. One clock per key keeps timer
/// replacement, hover pause and the error hard cap in one place.
struct ToastClock {
    remaining: Duration,
    running_since: Instant,
    paused: bool,
    hard_deadline: Option<Instant>,
    generation: u64,
}

impl ToastClock {
    fn new(lifetime_ms: u64, hard_cap_ms: Option<u64>, now: Instant) -> Self {
        Self {
            remaining: Duration::from_millis(lifetime_ms),
            running_since: now,
            paused: false,
            hard_deadline: hard_cap_ms.map(|ms| now + Duration::from_millis(ms)),
            generation: next_clock_generation(),
        }
    }
}

struct ToastEntry {
    /// Identifies the push that owns this entry. An old notification's close
    /// callback must not evict the entry a replacement already registered
    /// under the same key while the old one finishes its exit animation.
    instance: u64,
    clock: Option<ToastClock>,
    progress: Option<WeakEntity<ToastProgress>>,
    cleanup: Option<Box<dyn FnOnce() + Send + 'static>>,
}

fn toast_registry() -> &'static Mutex<HashMap<u64, ToastEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, ToastEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_toasts() -> std::sync::MutexGuard<'static, HashMap<u64, ToastEntry>> {
    toast_registry()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn run_cleanup(cleanup: Option<Box<dyn FnOnce() + Send + 'static>>) {
    if let Some(cleanup) = cleanup {
        cleanup();
    }
}

fn register_toast(key: u64, entry: ToastEntry) {
    let previous = lock_toasts().insert(key, entry);
    if let Some(previous) = previous {
        run_cleanup(previous.cleanup);
    }
}

/// The time until this toast's next reason to wake, plus the generation that
/// owns the answer. A paused toast only wakes for its hard cap; without one
/// it has no timer at all until the pointer leaves.
fn toast_wait(key: u64, now: Instant) -> Option<(u64, Duration)> {
    let map = lock_toasts();
    let clock = map.get(&key)?.clock.as_ref()?;
    let wait = if clock.paused {
        clock.hard_deadline?.saturating_duration_since(now)
    } else {
        let remaining = clock
            .remaining
            .saturating_sub(now.saturating_duration_since(clock.running_since));
        match clock.hard_deadline {
            Some(deadline) => remaining.min(deadline.saturating_duration_since(now)),
            None => remaining,
        }
    };
    Some((clock.generation, wait))
}

fn toast_due(key: u64, generation: u64, now: Instant) -> bool {
    let mut map = lock_toasts();
    let Some(clock) = map.get_mut(&key).and_then(|entry| entry.clock.as_mut()) else {
        return false;
    };
    if clock.generation != generation {
        return false;
    }
    if clock.hard_deadline.is_some_and(|deadline| now >= deadline) {
        return true;
    }
    if clock.paused {
        return false;
    }
    clock
        .remaining
        .saturating_sub(now.saturating_duration_since(clock.running_since))
        .is_zero()
}

fn schedule_toast_clock(key: u64, handle: AnyWindowHandle, cx: &mut App) {
    let now = cx.background_executor().now();
    let Some((generation, wait)) = toast_wait(key, now) else {
        return;
    };
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        executor.timer(wait).await;
        let _ = handle.update(cx, |_, window, cx| {
            if toast_due(key, generation, cx.background_executor().now()) {
                close_timed_toast(key, window, cx);
            }
        });
    })
    .detach();
}

fn close_timed_toast(key: u64, window: &mut Window, cx: &mut App) {
    if let Some(entry) = lock_toasts().remove(&key) {
        run_cleanup(entry.cleanup);
    }
    window.remove_notification1::<ToastLifetime>(("k5-toast", key as usize), cx);
}

fn pause_toast_clock(key: u64, now: Instant) -> bool {
    let mut map = lock_toasts();
    let Some(clock) = map.get_mut(&key).and_then(|entry| entry.clock.as_mut()) else {
        return false;
    };
    if clock.paused {
        return false;
    }
    clock.remaining = clock
        .remaining
        .saturating_sub(now.saturating_duration_since(clock.running_since));
    clock.paused = true;
    clock.generation = next_clock_generation();
    true
}

fn resume_toast_clock(key: u64, now: Instant) -> bool {
    let mut map = lock_toasts();
    let Some(clock) = map.get_mut(&key).and_then(|entry| entry.clock.as_mut()) else {
        return false;
    };
    if !clock.paused {
        return false;
    }
    clock.paused = false;
    clock.running_since = now;
    clock.generation = next_clock_generation();
    true
}

fn on_toast_hover(key: u64, hovered: bool, window: &mut Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let changed = if hovered {
        pause_toast_clock(key, now)
    } else {
        resume_toast_clock(key, now)
    };
    if changed {
        // A pause reschedules at the hard cap (if any); a resume reschedules
        // at whatever is left after subtracting the hovered span.
        schedule_toast_clock(key, window.window_handle(), cx);
    }
}

fn forget_toast(key: u64, instance: u64) {
    // Drop the registry guard before running cleanup: cleanup is arbitrary
    // caller code and must not execute under the lock.
    let entry = {
        let mut map = lock_toasts();
        if map
            .get(&key)
            .is_some_and(|entry| entry.instance == instance)
        {
            map.remove(&key)
        } else {
            None
        }
    };
    if let Some(entry) = entry {
        run_cleanup(entry.cleanup);
    }
}

fn action_button(action: &ToastAction) -> Button {
    let run = action.run.clone();
    Button::new(action.id)
        .label(action.label.clone())
        .small()
        .on_click(move |_, window, cx| run(window, cx))
}

fn toast_body(
    key: u64,
    message: SharedString,
    actions: Rc<Vec<ToastAction>>,
    progress: Option<Entity<ToastProgress>>,
) -> AnyElement {
    let single = actions.len() == 1;
    let pair = actions.len() > 1;

    v_flex()
        .id(("toast-body", key as usize))
        .test_support()
        .w_full()
        .gap_2()
        .on_hover(move |hovered, window, cx| on_toast_hover(key, *hovered, window, cx))
        .child(
            h_flex()
                .w_full()
                .items_start()
                .gap_3()
                .child(div().flex_1().min_w_0().text_sm().child(message))
                .when(single, |row| row.child(action_button(&actions[0]))),
        )
        .when_some(progress, |column, progress| column.child(progress))
        .when(pair, |column| {
            column.child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(action_button(&actions[0]))
                    .child(action_button(&actions[1])),
            )
        })
        .into_any_element()
}

fn build_toast_notification(
    key: u64,
    instance: u64,
    tone: ToastTone,
    message: String,
    actions: Vec<ToastAction>,
    progress: Option<Entity<ToastProgress>>,
) -> Notification {
    let message: SharedString = message.into();
    let actions = Rc::new(actions);
    tone.notification()
        .id1::<ToastLifetime>(("k5-toast", key as usize))
        .autohide(false)
        .on_close(move |_, _| forget_toast(key, instance))
        .content(move |_, _, _| toast_body(key, message.clone(), actions.clone(), progress.clone()))
}

/// Shared pusher for every app-timed toast. `Context<AppView>` and bare `App`
/// both reach it, because probe callbacks run inside window-handle updates
/// where only `&mut App` is at hand.
pub(crate) fn push_toast_spec(spec: ToastSpec, window: &mut Window, cx: &mut App) -> ToastHandle {
    let key = spec.key.unwrap_or_else(next_toast_key);
    let ToastSpec {
        tone,
        message,
        actions,
        progress,
        timed,
        lifetime_ms,
        hard_cap_ms,
        cleanup,
        ..
    } = spec;

    let now = cx.background_executor().now();
    let instance = next_clock_generation();
    let progress_entity = progress.map(|(done, total)| cx.new(|_| ToastProgress::new(done, total)));
    let clock = timed.then(|| ToastClock::new(lifetime_ms, hard_cap_ms, now));
    register_toast(
        key,
        ToastEntry {
            instance,
            clock,
            progress: progress_entity.as_ref().map(|entity| entity.downgrade()),
            cleanup,
        },
    );

    window.push_notification(
        build_toast_notification(key, instance, tone, message, actions, progress_entity),
        cx,
    );
    schedule_toast_clock(key, window.window_handle(), cx);
    ToastHandle { key, instance }
}

/// Update the ring without replacing the notification: the entity repaints,
/// the toast keeps its enter animation and its place in the stack.
pub(crate) fn update_toast_progress(handle: ToastHandle, done: usize, total: usize, cx: &mut App) {
    let progress = {
        let map = lock_toasts();
        let Some(entry) = map.get(&handle.key) else {
            return;
        };
        if entry.instance != handle.instance {
            return;
        }
        entry
            .progress
            .clone()
            .and_then(|progress| progress.upgrade())
    };
    if let Some(progress) = progress {
        progress.update(cx, |progress, cx| {
            // A group sweep's mux warm-up re-counts from the recorded rows, so
            // its first tick can be lower than the pass that just finished.
            // The ring only ever moves forward.
            let done = done.max(progress.done);
            if progress.done != done || progress.total != total {
                progress.done = done;
                progress.total = total;
                cx.notify();
            }
        });
    }
}

/// Test-only read of the live ring so the suite can pin the remaining-count
/// copy without reaching into a rendered text node.
#[cfg(test)]
pub(crate) fn toast_progress_label(key: u64, cx: &App) -> Option<String> {
    let progress = lock_toasts()
        .get(&key)
        .and_then(|entry| entry.progress.clone())
        .and_then(|progress| progress.upgrade())?;
    Some(progress.read(cx).label())
}

impl AppView {
    pub(crate) fn push_toast(
        &mut self,
        spec: ToastSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToastHandle {
        push_toast_spec(spec, window, cx)
    }

    pub(crate) fn toast_success(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.push_toast(ToastSpec::success(message), window, cx);
    }

    pub(crate) fn toast_error(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.push_toast(ToastSpec::error(message), window, cx);
    }
}

/// The `Testing N…` start toast. The key lives on the view, so each window
/// owns one probe toast and a sweep's completion closes exactly that one.
pub(crate) fn push_probe_start_toast(
    key: u64,
    message: String,
    done: usize,
    total: usize,
    window: &mut Window,
    cx: &mut App,
) -> ToastHandle {
    push_toast_spec(
        ToastSpec::info(message)
            .key(key)
            .no_timeout()
            .progress(done, total),
        window,
        cx,
    )
}

pub(crate) fn update_probe_progress(handle: ToastHandle, done: usize, total: usize, cx: &mut App) {
    update_toast_progress(handle, done, total, cx);
}

pub(crate) fn close_probe_start_toast(handle: ToastHandle, window: &mut Window, cx: &mut App) {
    let entry = {
        let mut map = lock_toasts();
        if map
            .get(&handle.key)
            .is_some_and(|entry| entry.instance == handle.instance)
        {
            map.remove(&handle.key)
        } else {
            None
        }
    };
    if let Some(entry) = entry {
        run_cleanup(entry.cleanup);
        window.remove_notification1::<ToastLifetime>(("k5-toast", handle.key as usize), cx);
    }
}

impl AppView {
    /// The delete → Undo toast. The button lives on the toast, so the removed
    /// rows are reachable until the toast itself expires; its timer both
    /// closes the toast and drops the stashed rows — expiry is permanent.
    /// K17: the caller builds the note, so a partial bulk delete can arrive
    /// as a warning (single-row keeps its success tint) — the Undo offer is
    /// unchanged.
    pub(crate) fn push_delete_undo_toast(
        &mut self,
        batch: u64,
        note: ToastSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let spec = note
            .key(batch)
            .lifetime(TOAST_UNDO_MS)
            .action("k5-undo", t!("toast-action-undo"), move |window, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.undo_expired_delete(batch, window, cx);
                });
            })
            .cleanup(move || drop_undo_batch(batch));
        self.push_toast(spec, window, cx);
    }

    /// K5: delete = immediate; Undo puts the removed rows back with their
    /// group assignments. Restore re-adds the rows through `add_batch`, never
    /// restarting engines: the row returns disabled, exactly as a manual
    /// re-add would.
    pub(crate) fn undo_expired_delete(
        &mut self,
        batch: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rows) = take_undo_rows(batch) else {
            return;
        };
        let names: Vec<String> = rows.iter().map(|row| row.name.clone()).collect();
        let owned_ports: HashSet<u16> = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.port)
            .collect();
        let inputs = rows
            .into_iter()
            .map(|row| {
                // The original port unless another row took it meanwhile —
                // an auto port beats a silent engine conflict.
                let taken = owned_ports.contains(&row.port);
                NewConfig {
                    name: row.name,
                    protocol: row.protocol,
                    params: row.params,
                    group: row.group,
                    bind_ip: row.bind_ip,
                    port: if taken {
                        String::new()
                    } else {
                        row.port.to_string()
                    },
                }
            })
            .collect();
        match self.store.add_batch(inputs) {
            Ok(restored) => {
                self.reload(window, cx);
                window.focus(&self.focus, cx);
                self.toast_success(undo_restored_message(&names, restored.len()), window, cx);
            }
            Err(error) => {
                self.toast_error(undo_failed_message(&error), window, cx);
            }
        }
    }
}
