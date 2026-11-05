use crate::*;

impl AppView {
    /// F18: the toolbar and the empty canvas share these two handlers.
    pub(crate) fn open_add_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_add_dialog(window, cx);
    }

    pub(crate) fn open_import_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
    }

    pub(crate) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let program_view = cx.entity();
        let groups_view = cx.entity();
        let routing_view = cx.entity();
        let tools_view = cx.entity();

        let running = self.engine.running_count();
        let busy = self.start_in_flight;
        let show_security = self.libraries.read(cx).delegate().show_security;
        let (glyph, name) = global_toggle_text(running, busy);
        let toggle_tip = if busy {
            t!("toolbar-starting-tip")
        } else if running == 0 {
            t!("toolbar-start-all-tip")
        } else {
            t!("toolbar-stop-all-tip")
        };
        div()
            .id("manyproxy-toolbar")
            .flex()
            .flex_row()
            .flex_nowrap()
            .items_center()
            .gap_1()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("program-menu")
                    .small()
                    .label(t!("toolbar-program"))
                    .accessibility_label(t!("toolbar-program"))
                    .tooltip(t!("toolbar-program-tip"))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "export-library",
                                    t!("toolbar-export"),
                                    t!("toolbar-export"),
                                    t!("toolbar-export-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = program_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_export_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "about-dialog",
                                    t!("toolbar-about"),
                                    t!("toolbar-about"),
                                    t!("toolbar-about-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = program_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_about_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                    }),
            )
            .child(
                Button::new("settings-menu")
                    .small()
                    .label(t!("action-settings"))
                    .accessibility_label(t!("action-settings"))
                    .tooltip(t!("toolbar-settings-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_settings_dialog(window, cx);
                    })),
            )
            .child(
                Button::new("groups-menu")
                    .small()
                    .label(t!("toolbar-library"))
                    .accessibility_label(t!("toolbar-library"))
                    .tooltip(t!("toolbar-library-tip"))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "subscriptions",
                                    t!("toolbar-subscriptions"),
                                    t!("toolbar-subscriptions"),
                                    t!("toolbar-subscriptions-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = groups_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_subscriptions_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "menu-groups-dialog",
                                    t!("toolbar-groups"),
                                    t!("toolbar-groups"),
                                    t!("toolbar-groups-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = groups_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_groups_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                let label = group_prune_label(GroupPruneKind::Duplicates);
                                topbar_menu_row(
                                    "prune-duplicates",
                                    label.clone(),
                                    label,
                                    t!("toolbar-prune-duplicates-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = groups_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_prune_confirm(
                                            GroupPruneKind::Duplicates,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                let label = group_prune_label(GroupPruneKind::Unavailable);
                                topbar_menu_row(
                                    "prune-unavailable",
                                    label.clone(),
                                    label,
                                    t!("toolbar-prune-unavailable-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = groups_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_prune_confirm(
                                            GroupPruneKind::Unavailable,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                let label = group_prune_label(GroupPruneKind::Invalid);
                                topbar_menu_row(
                                    "prune-invalid",
                                    label.clone(),
                                    label,
                                    t!("toolbar-prune-invalid-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = groups_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_prune_confirm(
                                            GroupPruneKind::Invalid,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                let label = group_prune_label(GroupPruneKind::ClearTests);
                                topbar_menu_row(
                                    "prune-clear-tests",
                                    label.clone(),
                                    label,
                                    t!("toolbar-prune-clear-tests-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = groups_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_prune_confirm(
                                            GroupPruneKind::ClearTests,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                        )
                    }),
            )
            .child(
                Button::new("routing-menu")
                    .small()
                    .label(t!("toolbar-routing"))
                    .accessibility_label(t!("toolbar-routing"))
                    .tooltip(t!("toolbar-routing-tip"))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "routing-presets",
                                    t!("toolbar-routing-presets"),
                                    t!("toolbar-routing-presets"),
                                    t!("toolbar-routing-presets-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = routing_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_presets_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "routing-attach-selected",
                                    t!("toolbar-attach-preset"),
                                    t!("toolbar-attach-preset"),
                                    t!("toolbar-attach-preset-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = routing_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_preset_chooser_for_selected(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "routing-open-settings",
                                    t!("action-settings"),
                                    t!("action-settings"),
                                    t!("toolbar-open-settings-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = routing_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_settings_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                    }),
            )
            .child(
                Button::new("tools-menu")
                    .small()
                    .label(t!("toolbar-tools"))
                    .accessibility_label(t!("toolbar-tools"))
                    .tooltip(t!("toolbar-tools-tip"))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "engine-start-all",
                                    t!("action-start-all"),
                                    t!("action-start-all"),
                                    t!("toolbar-start-all-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.start_all_engines(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "engine-stop-all",
                                    t!("action-stop-all"),
                                    t!("action-stop-all"),
                                    t!("toolbar-stop-all-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.stop_all_engines(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "health-test-all",
                                    t!("toolbar-test-all"),
                                    t!("toolbar-test-all"),
                                    t!("toolbar-test-all-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.test_all_health(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "stop-testing",
                                    t!("toolbar-stop-testing"),
                                    t!("toolbar-stop-testing"),
                                    t!("toolbar-stop-testing-tip"),
                                    Some(("ctrl-.", "(Ctrl-.)", "Ctrl-.")),
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.stop_group_tests(window, cx);
                                    });
                                }
                            }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "menu-logs",
                                    t!("toolbar-logs"),
                                    t!("toolbar-logs"),
                                    t!("toolbar-logs-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_logs_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "runtime-stats",
                                    t!("toolbar-runtime-stats"),
                                    t!("toolbar-runtime-stats"),
                                    t!("toolbar-runtime-stats-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_runtime_stats_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "traffic-stats",
                                    t!("toolbar-traffic-stats"),
                                    t!("toolbar-traffic-stats"),
                                    t!("toolbar-traffic-stats-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_traffic_stats_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "show-security",
                                    t!("toolbar-show-security"),
                                    t!("toolbar-show-security"),
                                    t!("toolbar-show-security-tip"),
                                    None,
                                )
                            })
                            .checked(show_security)
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.toggle_show_security(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "check-for-updates",
                                    t!("toolbar-check-updates"),
                                    t!("toolbar-check-updates"),
                                    t!("toolbar-check-updates-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.check_for_updates(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "menu-row-columns",
                                    t!("toolbar-row-columns"),
                                    t!("toolbar-row-columns"),
                                    t!("toolbar-row-columns-tip"),
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_row_columns_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "open-palette",
                                    t!("action-command-palette"),
                                    t!("action-command-palette"),
                                    t!("toolbar-palette-tip"),
                                    Some(("ctrl-k", "(Ctrl-K)", "Ctrl-K")),
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_palette(window, cx);
                                    });
                                }
                            }),
                        )
                    }),
            )
            .child(div().flex_1())
            // T-03: the two intake actions ride the top right: Add is the
            // primary commitment, Import sits beside it.
            .child(
                Button::new("toolbar-import")
                    .small()
                    .label(t!("action-import"))
                    .accessibility_label(t!("action-import"))
                    .tooltip(t!("toolbar-import-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_import_config(window, cx);
                    })),
            )
            .child(
                Button::new("toolbar-add-config")
                    .small()
                    .primary()
                    .label(t!("action-add-config"))
                    .accessibility_label(t!("action-add-config-accessible"))
                    .tooltip(t!("action-add-config-tip"))
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_add_config(window, cx);
                    })),
            )
            .child(
                Button::new("global-start-stop")
                    .small()
                    .label(format!("{glyph} {name}"))
                    .accessibility_label(name)
                    .loading(busy)
                    .tooltip(toggle_tip)
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_global_engines(window, cx);
                    })),
            )
    }
}
