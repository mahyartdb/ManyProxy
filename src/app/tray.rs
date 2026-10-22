use crate::*;

impl AppView {
    pub(crate) fn poll_tray_once(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.tray.is_none() {
            return false;
        }

        let mut actions = Vec::new();
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if let Some(action) = crate::tray::action_for_menu_id(event.id.as_ref()) {
                actions.push(action);
            }
        }
        if actions.contains(&crate::tray::TrayAction::Quit) {
            self.quit_from_tray(window);
            return true;
        }
        for action in actions {
            self.handle_tray_action(action, window, cx);
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                }
            ) {
                self.handle_tray_action(crate::tray::TrayAction::Show, window, cx);
            }
        }
        false
    }

    pub(crate) fn handle_tray_action(
        &mut self,
        action: crate::tray::TrayAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            crate::tray::TrayAction::Show => self.show_window_from_tray(window, cx),
            crate::tray::TrayAction::SelectExit => {
                self.show_window_from_tray(window, cx);
                self.open_tray_exit_picker(window, cx);
            }
            crate::tray::TrayAction::SelectPreset => {
                self.show_window_from_tray(window, cx);
                self.open_presets_dialog(window, cx);
            }
            crate::tray::TrayAction::StartAll => {
                self.show_window_from_tray(window, cx);
                self.start_all_engines(window, cx);
            }
            crate::tray::TrayAction::StopAll => {
                self.show_window_from_tray(window, cx);
                self.stop_all_engines(window, cx);
            }
            crate::tray::TrayAction::OpenConfig => {
                self.show_window_from_tray(window, cx);
                self.copy_config_folder(window, cx);
            }
            crate::tray::TrayAction::Quit => self.quit_from_tray(window),
        }
    }
}
