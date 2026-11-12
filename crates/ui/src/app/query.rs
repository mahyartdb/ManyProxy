use crate::*;

impl AppView {
    /// The read-only mirrors the pure query counts against.
    pub(crate) fn query_facts<'a>(&self, delegate: &'a LibraryDelegate) -> QueryFacts<'a> {
        QueryFacts {
            running: &delegate.running,
            health: &delegate.health,
            crashed: &delegate.crashed,
            latency: &delegate.latency,
        }
    }

    /// Recompute the visible indices from the one query. The delegate keeps
    /// the row mirrors fresh; this is the only writer of `AppView::visible`.
    pub(crate) fn apply_query(&mut self, cx: &mut Context<Self>) {
        let visible = {
            let libraries = self.libraries.read(cx);
            let delegate = libraries.delegate();
            let facts = QueryFacts {
                running: &delegate.running,
                health: &delegate.health,
                crashed: &delegate.crashed,
                latency: &delegate.latency,
            };
            let mut visible = filter(
                &delegate.rows,
                &delegate.search_index,
                &facts,
                &self.scope,
                &self.query,
            );
            // R29: the batch failure filter is a slice-2 narrowing, not a
            // facet; it still participates in N and the paint order.
            if let Some(failed) = &self.failure_filter {
                visible.retain(|&index| {
                    delegate
                        .rows
                        .get(index)
                        .is_some_and(|row| failed.contains(&row.id))
                });
            }
            if !delegate.sort.is_none() {
                if delegate.sort.key == LibrarySortKey::TypeSecurity {
                    visible.sort_by(|&a, &b| {
                        compare_type_security(&delegate.rows[a], &delegate.rows[b])
                    });
                } else {
                    let context = SortContext {
                        latency: &delegate.latency,
                        data_traffic: &delegate.data_traffic,
                        totals: &delegate.traffic,
                        out_ips: &delegate.out_ips,
                    };
                    visible.sort_by(|&a, &b| {
                        compare_library_rows(
                            &context,
                            delegate.sort,
                            &delegate.rows[a],
                            &delegate.rows[b],
                        )
                    });
                }
            }
            visible
        };
        self.visible = visible;
    }

    /// One keystroke schedules the 80ms task; a newer keystroke retires it by
    /// epoch and by dropping the stored task.
    pub(crate) fn schedule_search(&mut self, text: String, cx: &mut Context<Self>) {
        self.search_epoch = self.search_epoch.wrapping_add(1);
        let epoch = self.search_epoch;
        let weak = cx.entity().downgrade();
        self.search_debounce = Some(cx.spawn(async move |_this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            let _ = weak.update(cx, |view, cx| {
                if view.search_epoch != epoch {
                    return;
                }
                let before = view.query.port;
                view.query.set_search(text);
                view.port_fields_stale |= view.query.port != before;
                view.failure_filter = None;
                view.apply_query(cx);
                cx.notify();
            });
        }));
    }

    /// Enter, a cleared field and `Clear` apply without waiting for the timer.
    pub(crate) fn apply_search_now(&mut self, text: String, cx: &mut Context<Self>) {
        self.search_debounce = None;
        self.search_epoch = self.search_epoch.wrapping_add(1);
        let before = self.query.port;
        self.query.set_search(text);
        self.port_fields_stale |= self.query.port != before;
        self.failure_filter = None;
        self.apply_query(cx);
        cx.notify();
    }

    /// The visible rows' ids, in paint order. The one reader for the batch
    /// actions and the selection helpers.
    pub(crate) fn visible_row_ids(&self, cx: &App) -> Vec<String> {
        let delegate = self.libraries.read(cx).delegate();
        self.visible
            .iter()
            .filter_map(|&index| delegate.rows.get(index))
            .map(|row| row.id.clone())
            .collect()
    }

    pub(crate) fn set_status_facet(
        &mut self,
        status: StatusFacet,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        if on {
            self.query.statuses.insert(status);
        } else {
            self.query.statuses.remove(&status);
        }
        self.query_edited(cx);
    }

    pub(crate) fn set_protocol_facet(
        &mut self,
        protocol: String,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        if on {
            self.query.protocols.insert(protocol);
        } else {
            self.query.protocols.remove(&protocol);
        }
        self.query_edited(cx);
    }

    pub(crate) fn set_bind_facet(&mut self, bind: String, on: bool, cx: &mut Context<Self>) {
        if on {
            self.query.binds.insert(bind);
        } else {
            self.query.binds.remove(&bind);
        }
        self.query_edited(cx);
    }

    /// The latency facet is session-only; the old store write is gone.
    pub(crate) fn set_latency_facet(&mut self, facet: LatencyFacet, cx: &mut Context<Self>) {
        self.query.latency = facet;
        self.query_edited(cx);
    }

    /// The two port fields and a `port=` token are views of one value. Invalid
    /// text keeps the previous filter and shows the inline error.
    pub(crate) fn set_port_range(&mut self, from: &str, to: &str, cx: &mut Context<Self>) {
        let from = from.trim();
        let to = to.trim();
        let parsed = if from.is_empty() && to.is_empty() {
            Ok(None)
        } else {
            let lo = if from.is_empty() {
                Ok(0)
            } else {
                from.parse::<u16>().map_err(|_| ())
            };
            let hi = if to.is_empty() {
                Ok(u16::MAX)
            } else {
                to.parse::<u16>().map_err(|_| ())
            };
            match (lo, hi) {
                (Ok(lo), Ok(hi)) if lo <= hi => Ok(Some(PortPredicate { lo, hi })),
                _ => Err(()),
            }
        };
        match parsed {
            Ok(port) => {
                self.port_error = None;
                if self.query.port == port {
                    cx.notify();
                    return;
                }
                self.query.port = port;
                self.query_edited(cx);
            }
            Err(()) => {
                self.port_error = Some(t!("filter-port-invalid"));
                cx.notify();
            }
        }
    }

    pub(crate) fn set_custom_endpoint(&mut self, on: bool, cx: &mut Context<Self>) {
        self.query.custom_endpoint = on;
        self.query_edited(cx);
    }

    /// The protocol popover's local filter text changed; the popover content
    /// re-renders from the input's value.
    pub(crate) fn protocol_filter_changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// A control edit takes the slice-2 failure narrowing away, then recomputes.
    fn query_edited(&mut self, cx: &mut Context<Self>) {
        self.failure_filter = None;
        self.apply_query(cx);
        cx.notify();
    }

    /// Clear the field. A `port=` token in the field goes with it; a port set
    /// from the popover stays when the field was already empty.
    pub(crate) fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_debounce = None;
        self.search_epoch = self.search_epoch.wrapping_add(1);
        let before = self.query.port;
        self.query.set_search("");
        self.port_fields_stale |= self.query.port != before;
        self.failure_filter = None;
        self.write_search_box("", window, cx);
        self.apply_query(cx);
        cx.notify();
    }

    /// `Clear`: search and every facet.
    pub(crate) fn clear_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_debounce = None;
        self.search_epoch = self.search_epoch.wrapping_add(1);
        self.query = FilterQuery::default();
        self.failure_filter = None;
        self.port_error = None;
        self.write_search_box("", window, cx);
        self.sync_port_fields(window, cx);
        self.apply_query(cx);
        cx.notify();
    }

    /// Esc after the selection is empty: filters go, the search text stays.
    pub(crate) fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.query.statuses.clear();
        self.query.protocols.clear();
        self.query.latency = LatencyFacet::default();
        self.query.binds.clear();
        self.query.port = None;
        self.query.custom_endpoint = false;
        self.port_error = None;
        self.port_fields_stale = true;
        self.failure_filter = None;
        self.apply_query(cx);
        cx.notify();
    }

    pub(crate) fn write_search_box(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_input.update(cx, |input, cx| {
            input.set_value(text.to_string(), window, cx);
        });
    }

    /// The Filter popover shows the current port; a `port=` token seeds it.
    pub(crate) fn sync_port_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (from, to) = match self.query.port {
            Some(port) => (
                if port.lo == 0 {
                    String::new()
                } else {
                    port.lo.to_string()
                },
                if port.hi == u16::MAX {
                    String::new()
                } else {
                    port.hi.to_string()
                },
            ),
            None => (String::new(), String::new()),
        };
        if self.port_from.read(cx).value().as_str() != from {
            self.port_from.update(cx, |input, cx| {
                input.set_value(from, window, cx);
            });
        }
        if self.port_to.read(cx).value().as_str() != to {
            self.port_to.update(cx, |input, cx| {
                input.set_value(to, window, cx);
            });
        }
    }
}
