use crate::*;

impl AppView {
    pub(crate) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let program_view = cx.entity();
        let settings_view = cx.entity();
        let groups_view = cx.entity();
        let routing_view = cx.entity();
        let tools_view = cx.entity();

        let running = self.engine.running_count();
        let busy = self.start_in_flight;
        let (glyph, name) = global_toggle_present(running, busy);
        let toggle_tip = if busy {
            "Starting every endpoint…"
        } else if running == 0 {
            "Start every configuration"
        } else {
            "Stop every endpoint"
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
                    .label("Program")
                    .accessibility_label("Program")
                    .tooltip("App and single-config intake")
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "menu-add-config",
                                    "Add configuration",
                                    "Add configuration",
                                    "Add a proxy configuration",
                                    Some(("ctrl-n", "(Ctrl-N)", "Ctrl-N")),
                                )
                            })
                            .on_click({
                                let view = program_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_add_dialog(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "menu-import-config",
                                    "Import",
                                    "Import",
                                    "Import share links or files",
                                    Some(("ctrl-o", "(Ctrl-O)", "Ctrl-O")),
                                )
                            })
                            .on_click({
                                let view = program_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_import_dialog(
                                            "",
                                            DEFAULT_GROUP,
                                            None,
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "import-clipboard",
                                    "From clipboard",
                                    "From clipboard",
                                    "Import from clipboard text",
                                    Some(("ctrl-v", "(Ctrl-V)", "Ctrl-V")),
                                )
                            })
                            .on_click({
                                let view = program_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_import_from_clipboard(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "import-files",
                                    "From file(s)",
                                    "From file(s)",
                                    "Pick config files to import (any type, 50MB each)",
                                    None,
                                )
                            })
                            .on_click({
                                let view = program_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.browse_for_import_files(
                                            String::new(),
                                            DEFAULT_GROUP.to_string(),
                                            window,
                                            cx,
                                        );
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "export-library",
                                    "Export",
                                    "Export",
                                    "Export library to JSON",
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
                                    "About",
                                    "About",
                                    "Version, sidecars, shortcuts",
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
                    .label("Settings")
                    .accessibility_label("Settings")
                    .tooltip("Settings")
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "settings-dialog",
                                    "Settings",
                                    "Settings",
                                    "Autostart, restore, test URL",
                                    None,
                                )
                            })
                            .on_click({
                                let view = settings_view.clone();
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
                Button::new("groups-menu")
                    .small()
                    .label("Manage groups…")
                    .accessibility_label("Manage groups…")
                    .tooltip("Subscriptions and groups")
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "subscriptions",
                                    "Subscriptions",
                                    "Subscriptions",
                                    "Manage subscription lists",
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
                                    "Groups",
                                    "Groups",
                                    "Rename or merge groups",
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
                                topbar_menu_row(
                                    "prune-duplicates",
                                    GroupPruneKind::Duplicates.label(),
                                    GroupPruneKind::Duplicates.label(),
                                    "Remove later copies in the focused group (asks first)",
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
                                topbar_menu_row(
                                    "prune-unavailable",
                                    GroupPruneKind::Unavailable.label(),
                                    GroupPruneKind::Unavailable.label(),
                                    "Remove dead rows in the focused group (asks first)",
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
                                topbar_menu_row(
                                    "prune-invalid",
                                    GroupPruneKind::Invalid.label(),
                                    GroupPruneKind::Invalid.label(),
                                    "Remove validation-failing rows in the focused group (asks first)",
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
                                topbar_menu_row(
                                    "prune-clear-tests",
                                    GroupPruneKind::ClearTests.label(),
                                    GroupPruneKind::ClearTests.label(),
                                    "Clear test results in the focused group (rows kept, asks first)",
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
                    .label("Routing")
                    .accessibility_label("Routing")
                    .tooltip("Routing presets and settings")
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "routing-presets",
                                    "Routing presets",
                                    "Routing presets",
                                    "Manage routing presets",
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
                                    "Attach preset to selected row",
                                    "Attach preset to selected row",
                                    "Pick a routing preset for the selected row",
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
                                    "Routing settings",
                                    "Routing settings",
                                    "Open Settings at the Routing section",
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
                    .label("Tools")
                    .accessibility_label("Tools")
                    .tooltip("Selection, diagnostics, and commands")
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "start-selected",
                                    "Start selected",
                                    "Start selected",
                                    "Start the selected endpoint",
                                    Some(("f5", "(F5)", "F5")),
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.start_selected_engine(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "stop-selected",
                                    "Stop selected",
                                    "Stop selected",
                                    "Stop the selected endpoint",
                                    Some(("shift-f5", "(Shift-F5)", "Shift-F5")),
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.stop_selected_engine(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "test-selected",
                                    "Test selected",
                                    "Test selected",
                                    "Test the selected endpoint",
                                    Some(("ctrl-t", "(Ctrl-T)", "Ctrl-T")),
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.test_selected_health(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row("menu-logs", "Logs", "Logs", "Sidecar output", None)
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
                                    "Runtime stats",
                                    "Runtime stats",
                                    "Process health on one screen",
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
                                    "Traffic stats",
                                    "Traffic stats",
                                    "Per-exit traffic with period and By Profile",
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
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "engine-start-all",
                                    "Start all",
                                    "Start all",
                                    "Start every configuration",
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
                                    "Stop all",
                                    "Stop all",
                                    "Stop every endpoint",
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
                                    "Test all",
                                    "Test all",
                                    "Test every running endpoint",
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
                                    "test-group",
                                    "Test group",
                                    "Test group",
                                    "URL-test the focused group's running endpoints",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.test_group_health(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "speedtest-selected",
                                    "Speedtest selected",
                                    "Speedtest selected",
                                    "Measure the selected endpoint's transfer rate",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.speedtest_selected(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "speedtest-group",
                                    "Speedtest group",
                                    "Speedtest group",
                                    "Measure the focused group's running endpoints",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.speedtest_group(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "speedtest-mode",
                                    "Speedtest mode",
                                    "Speedtest mode",
                                    "Cycle download → upload (timeout stays 5s)",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.cycle_speedtest_mode(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "out-ip-selected",
                                    "Resolve Out-IP selected",
                                    "Resolve Out-IP selected",
                                    "Show the selected row's egress IP address",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.resolve_out_ip_selected(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "out-ip-group",
                                    "Resolve Out-IP group",
                                    "Resolve Out-IP group",
                                    "Show the focused group's egress IP addresses",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.resolve_out_ip_group(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "resolve-domain-group",
                                    "Resolve domain group",
                                    "Resolve domain group",
                                    "Rewrite the focused group's domain servers to IPs (asks first)",
                                    None,
                                )
                            })
                            .on_click({
                                let view = tools_view.clone();
                                move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.open_resolve_domain_group_confirm(window, cx);
                                    });
                                }
                            }),
                        )
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "stop-testing",
                                    "Stop Testing",
                                    "Stop Testing",
                                    "Stop the running group test",
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
                        .item(
                            PopupMenuItem::element(|_, _| {
                                topbar_menu_row(
                                    "check-for-updates",
                                    "Check for updates",
                                    "Check for updates",
                                    "Fetch the release feed once (JSON only — nothing downloads or runs)",
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
                                    "show-security",
                                    "Show security",
                                    "Show security",
                                    "Toggle the transport-security column",
                                    None,
                                )
                            })
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
                                    "menu-row-columns",
                                    "Row columns…",
                                    "Row columns",
                                    "Choose which columns each library row shows",
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
                                    "Command palette",
                                    "Command palette",
                                    "Every action, searchable (Ctrl-K)",
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
                    .label("Import")
                    .accessibility_label("Import")
                    .tooltip("Import share links or files (Ctrl-O)")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_import_dialog("", DEFAULT_GROUP, None, window, cx);
                    })),
            )
            .child(
                Button::new("toolbar-add-config")
                    .small()
                    .primary()
                    .label("+ Add configuration")
                    .accessibility_label("Add configuration")
                    .tooltip("Add a proxy configuration (Ctrl-N)")
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.open_add_dialog(window, cx);
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
