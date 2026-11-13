//! The one selection model: the set, the anchor and the focus.
//!
//! Belongs: `AppView`'s selection state and every gesture that writes it.
//! Not here: the row's own painting, or the batch that acts on a snapshot.
//!
//! Three facts, three owners of the truth: the set says what is selected,
//! the anchor says where a range grows from, the focus says where the
//! keyboard is. Running, selected and focused stay independent reads.

use crate::*;

/// What a batch did to the rows it snapshotted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchKind {
    Start,
    Stop,
    Test,
}

/// One batch's snapshot and its result. Transient: it lives until the
/// selection changes or the user clears the result.
#[derive(Debug, Clone)]
pub struct BatchRun {
    /// The verb the batch ran.
    pub kind: BatchKind,
    /// The ids the batch acts on, snapshotted at invocation. A selection
    /// change mid-flight never changes this list.
    pub ids: Vec<String>,
    /// How many rows the batch has finished.
    pub done: usize,
    /// How many rows succeeded.
    pub ok: usize,
    /// The ids that failed.
    pub failed: Vec<String>,
}

impl BatchRun {
    pub fn new(kind: BatchKind, ids: Vec<String>) -> Self {
        Self {
            kind,
            ids,
            done: 0,
            ok: 0,
            failed: Vec::new(),
        }
    }

    pub fn total(&self) -> usize {
        self.ids.len()
    }

    /// The failed ids, in the order the batch ran them.
    pub fn failed_ids(&self) -> Vec<String> {
        self.failed.clone()
    }

    /// The batch has a verdict for every id it snapshotted.
    pub fn is_finished(&self) -> bool {
        self.done >= self.total()
    }
}

impl AppView {
    /// Whether a row is in the selection.
    pub fn is_selected(&self, id: &str) -> bool {
        self.selection.contains(id)
    }

    /// Whether a row holds the keyboard focus.
    pub fn is_focused(&self, id: &str) -> bool {
        self.selection_focus.as_deref() == Some(id)
    }

    /// How many rows the selection holds.
    pub fn selection_count(&self) -> usize {
        self.selection.len()
    }

    /// The selected ids in the order the table paints them, with the rows a
    /// filter hides appended in library order. Selection is not a visible-row
    /// set: a hidden row stays selected, so it still counts and still acts.
    pub fn selected_ids_in_visible_order(&self, cx: &App) -> Vec<String> {
        let mut ids: Vec<String> = self
            .visible_row_ids(cx)
            .into_iter()
            .filter(|id| self.selection.contains(id))
            .collect();
        let mut hidden: Vec<String> = self
            .libraries
            .read(cx)
            .delegate()
            .row_ids()
            .into_iter()
            .filter(|id| self.selection.contains(id))
            .filter(|id| !ids.contains(id))
            .collect();
        ids.append(&mut hidden);
        ids
    }

    /// How many selected rows the current filter hides. The count is pure so
    /// the batch bar and the list can never disagree.
    pub fn selection_hidden_count(&self, cx: &App) -> usize {
        let delegate = self.libraries.read(cx).delegate();
        manyproxy_core::library::query::selection_hidden_count(
            &self.selection,
            &self.visible,
            &delegate.rows,
        )
    }

    /// Plain click: the selection becomes exactly this row, and the anchor and
    /// the focus follow it.
    pub fn select_row(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selection_changed();
        self.selection.clear();
        self.selection.insert(id.to_string());
        self.selection_anchor = Some(id.to_string());
        self.selection_focus = Some(id.to_string());
        cx.notify();
    }

    /// Ctrl/Cmd click: the row joins or leaves the set. The focus follows it;
    /// the anchor stays where it was unless the set became empty.
    pub fn toggle_row(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selection_changed();
        if !self.selection.remove(id) {
            self.selection.insert(id.to_string());
        }
        self.selection_focus = Some(id.to_string());
        if self.selection.is_empty() {
            self.selection_anchor = None;
        }
        cx.notify();
    }

    /// Shift click: the visible range from the anchor to the clicked row
    /// replaces the set. The focus becomes the clicked row; the anchor stays
    /// where it was, so the range can be re-dragged.
    pub fn extend_to_row(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selection_changed();
        let visible = self.visible_row_ids(cx);
        let anchor = self
            .selection_anchor
            .clone()
            .unwrap_or_else(|| id.to_string());
        let Some(from) = visible.iter().position(|row| row == &anchor) else {
            self.select_row(id, cx);
            return;
        };
        let Some(to) = visible.iter().position(|row| row == id) else {
            self.select_row(id, cx);
            return;
        };
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        self.selection = visible[lo..=hi].iter().cloned().collect();
        self.selection_anchor = Some(anchor);
        self.selection_focus = Some(id.to_string());
        cx.notify();
    }

    /// Ctrl+A: every row the filters keep. The anchor becomes the first row;
    /// the focus stays where it was, or becomes the first.
    pub fn select_all_visible_rows(&mut self, cx: &mut Context<Self>) {
        self.selection_changed();
        let visible = self.visible_row_ids(cx);
        self.selection = visible.iter().cloned().collect();
        self.selection_anchor = visible.first().cloned();
        if self.selection_focus.is_none() {
            self.selection_focus = visible.first().cloned();
        }
        cx.notify();
    }

    /// The selection, the anchor and the focus all go, and the keyboard goes
    /// back to the shell: with no row selected there is nothing for the
    /// list's key context to drive.
    pub fn clear_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_selection(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Drop every trace of the selection without moving the keyboard focus.
    /// `clear_selection` adds the focus move; `refresh_scope_index` uses this
    /// when a scope's bucket disappeared and it falls back to `All`.
    pub fn reset_selection(&mut self, cx: &mut Context<Self>) {
        self.selection_changed();
        self.selection.clear();
        self.selection_anchor = None;
        self.selection_focus = None;
        cx.notify();
    }

    /// A scope change drops the selection, the anchor and the focus: the rows
    /// the old scope held are not in the new one.
    pub fn clear_selection_for_scope_change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_selection(window, cx);
    }

    /// Prune the selection to the rows that still exist. A row that stops
    /// existing leaves the set; a row that is merely filtered out stays.
    pub fn prune_selection_to_live(
        &mut self,
        live_ids: &HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        self.selection_changed();
        self.selection.retain(|id| live_ids.contains(id));
        if self
            .selection_anchor
            .as_deref()
            .is_some_and(|id| !live_ids.contains(id))
        {
            self.selection_anchor = None;
        }
        if self
            .selection_focus
            .as_deref()
            .is_some_and(|id| !live_ids.contains(id))
        {
            self.selection_focus = None;
        }
        if self.selection.is_empty() {
            self.selection_anchor = None;
            self.selection_focus = None;
        }
        cx.notify();
    }

    /// Prune the selection against the rows the library holds right now.
    pub fn prune_selection_to_live_rows(&mut self, cx: &mut Context<Self>) {
        let live: HashSet<String> = {
            let libraries = self.libraries.read(cx);
            libraries.delegate().row_ids().into_iter().collect()
        };
        self.prune_selection_to_live(&live, cx);
    }

    /// The visible row at an index, by position. The tests drive the
    /// selection by position; the app itself only ever names ids.
    #[cfg(test)]
    pub fn select_visible_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(id) = self.visible_row_ids(cx).get(index).cloned() else {
            return;
        };
        self.select_row(&id, cx);
    }

    /// The tests name the rows they mean, without a pointer.
    #[cfg(test)]
    pub fn select_rows(&mut self, ids: &[String], cx: &mut Context<Self>) {
        self.selection_changed();
        self.selection.clear();
        for id in ids {
            self.selection.insert(id.clone());
        }
        self.selection_anchor = ids.first().cloned();
        self.selection_focus = ids.first().cloned();
        cx.notify();
    }

    /// The selection's ids, sorted, so a test can compare them.
    #[cfg(test)]
    pub fn selection_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.selection.iter().cloned().collect();
        ids.sort();
        ids
    }

    /// A selection change drops a finished batch's result: those numbers
    /// belong to the rows the batch snapshotted, not to whatever is selected
    /// now. A batch still in flight keeps reporting, so a mid-flight click
    /// cannot hide the outcome it is about to land.
    pub fn selection_changed(&mut self) {
        if self.batch.as_ref().is_some_and(BatchRun::is_finished) {
            self.batch = None;
        }
    }

    /// Space: the focused row joins or leaves the set.
    pub fn toggle_focused_row(&mut self, cx: &mut Context<Self>) {
        let Some(focus) = self.selection_focus.clone() else {
            return;
        };
        self.toggle_row(&focus, cx);
    }

    /// The focus moves one visible row up or down, and the selection becomes
    /// that row alone.
    pub fn move_focus(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.focus_target(delta, cx) else {
            return;
        };
        self.select_row(&target, cx);
        self.scroll_to_row(&target, window, cx);
    }

    /// Shift+Up/Down: the focus moves one visible row and the range from the
    /// anchor to the new focus replaces the set.
    pub fn extend_focus(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.focus_target(delta, cx) else {
            return;
        };
        self.extend_to_row(&target, cx);
        self.scroll_to_row(&target, window, cx);
    }

    /// Home: the first visible row takes the focus and the selection.
    pub fn focus_first_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.visible_row_ids(cx).first().cloned() else {
            return;
        };
        self.select_row(&target, cx);
        self.scroll_to_row(&target, window, cx);
    }

    /// End: the last visible row takes the focus and the selection.
    pub fn focus_last_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.visible_row_ids(cx).last().cloned() else {
            return;
        };
        self.select_row(&target, cx);
        self.scroll_to_row(&target, window, cx);
    }

    /// The visible row the focus moves to. Without a focus the movement
    /// enters at the top (down) or the bottom (up).
    pub fn focus_target(&self, delta: isize, cx: &App) -> Option<String> {
        let visible = self.visible_row_ids(cx);
        if visible.is_empty() {
            return None;
        }
        let current = self
            .selection_focus
            .as_deref()
            .and_then(|id| visible.iter().position(|row| row == id));
        let index = match current {
            Some(index) => (index as isize + delta).clamp(0, visible.len() as isize - 1) as usize,
            None if delta < 0 => visible.len() - 1,
            None => 0,
        };
        visible.get(index).cloned()
    }

    /// Scroll the focused row into view through the list's own API.
    pub fn scroll_to_row(&self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.visible_row_ids(cx).iter().position(|row| row == id);
        let Some(index) = index else {
            return;
        };
        self.libraries.update(cx, |state, list_cx| {
            state.scroll_to_item(
                IndexPath::new(index),
                ScrollStrategy::Nearest,
                window,
                list_cx,
            );
        });
    }
}
