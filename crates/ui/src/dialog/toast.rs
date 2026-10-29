use crate::*;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::time::Duration;

/// K5: the whole toast surface speaks one grammar — park bottom-right above
/// the status bar, at most two visible, and dismiss on our own clock so a
/// success toast (~4s) beats an error toast (~8s). `autohide(false)` plus a
/// timer closed by `remove_notification1` is the only route to a non-default
/// lifetime, because the kit itself bakes 5s into `NotificationList::push`.
pub(crate) struct ToastLifetime;
/// Marker for the `Testing N…` start toast: one fixed key so a repeat sweep
/// replaces it instead of stacking.
pub(crate) struct ProbeStartToast;

/// ~4s for success toasts (spec: ~4s success).
pub(crate) const TOAST_SUCCESS_MS: u64 = 4200;
/// ~8s for error toasts (spec: ~8s error).
pub(crate) const TOAST_ERROR_MS: u64 = 8000;
/// Delete→Undo toast lifetime; the removed rows live in memory exactly as long.
pub(crate) const TOAST_UNDO_MS: u64 = 8000;
/// The status bar is ~26px tall; a cushion above it is the small safe-area
/// offset, so a toast can never cover the bar or sit on the window edge.
const TOAST_BOTTOM_MARGIN: f32 = 36.0;
const TOAST_EDGE_MARGIN: f32 = 16.0;

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
static NEXT_UNDO_BATCH: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_undo_batch_id() -> u64 {
    NEXT_UNDO_BATCH.fetch_add(1, Ordering::SeqCst)
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

impl ProbeStartToast {
    pub(crate) fn element_key() -> ElementId {
        ("k5-probe-start", 0usize).into()
    }
}

impl AppView {
    /// Shared pusher for K5 toasts that outlive the kit's fixed 5s autohide:
    /// no autohide, one instance-wide key, and a timer that closes exactly
    /// that instance, so stacked toasts never fight over lifetimes.
    fn push_timed_toast(
        &mut self,
        note: Notification,
        lifetime_ms: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        let key = next_undo_batch_id();
        self.push_timed_toast_with_key(note, key, lifetime_ms, window, cx);
        key
    }

    fn push_timed_toast_with_key(
        &mut self,
        note: Notification,
        key: u64,
        lifetime_ms: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let element_id: ElementId = ("k5-toast", key as usize).into();
        let note = note
            .id1::<ToastLifetime>(("k5-toast", key as usize))
            .autohide(false);
        window.push_notification(note, cx);
        let handle = window.window_handle();
        cx.spawn(async move |_entity, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(lifetime_ms))
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                window.remove_notification1::<ToastLifetime>(element_id, cx);
            });
        })
        .detach();
    }

    pub(crate) fn toast_success(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note = Notification::success(message);
        self.push_timed_toast(note, TOAST_SUCCESS_MS, window, cx);
    }

    pub(crate) fn toast_error(
        &mut self,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note = Notification::error(message);
        self.push_timed_toast(note, TOAST_ERROR_MS, window, cx);
    }
}

/// The `Testing N…` start toast. Fixed key per kind: probe batches of one
/// kind are serialized by `probe_kind_in_flight`, and the sweep's own
/// completion closes it, so no timer ever races a replacement. Free functions
/// because both live inside window-handle updates, where only `&mut App` is
/// at hand.
pub(crate) fn push_probe_start_toast(message: String, window: &mut Window, cx: &mut App) {
    let note = Notification::info(message)
        .id1::<ProbeStartToast>(ProbeStartToast::element_key())
        .autohide(false);
    window.push_notification(note, cx);
}

pub(crate) fn close_probe_start_toast(window: &mut Window, cx: &mut App) {
    window.remove_notification1::<ProbeStartToast>(ProbeStartToast::element_key(), cx);
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
        note: Notification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let note = note
            .id1::<ToastLifetime>(("k5-toast", batch as usize))
            .action(move |_, _, _| {
                let view = view.clone();
                Button::new("k5-undo").label("Undo").on_click(
                    move |_, window: &mut Window, cx: &mut App| {
                        let _ = view.update(cx, |view, cx| {
                            view.undo_expired_delete(batch, window, cx);
                        });
                    },
                )
            })
            .autohide(false);
        window.push_notification(note, cx);
        let handle = window.window_handle();
        cx.spawn(async move |_entity, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(TOAST_UNDO_MS))
                .await;
            drop_undo_batch(batch);
            let _ = handle.update(cx, |_, window, cx| {
                window.remove_notification1::<ToastLifetime>(("k5-toast", batch as usize), cx);
            });
        })
        .detach();
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
                window.push_notification(
                    Notification::success(undo_restored_message(&names, restored.len())),
                    cx,
                );
            }
            Err(error) => {
                self.toast_error(undo_failed_message(&error), window, cx);
            }
        }
    }
}
