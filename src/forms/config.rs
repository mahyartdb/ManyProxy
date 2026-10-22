use crate::*;

pub(crate) fn advanced_settings_button(
    view: &Entity<AppView>,
    form: &ConfigForm,
) -> impl IntoElement {
    let adv_view = view.clone();
    let adv_form = form.clone();
    Button::new("advanced-settings")
        .small()
        .label("Advanced Settings")
        .tooltip("Dial fields and TLS extras — edits apply directly, no extra checks")
        .accessibility_label("Open Advanced Settings: dial fields and TLS extras")
        .on_click(move |_, window, cx| {
            adv_view.update(cx, |view, cx| {
                view.open_advanced_dialog(&adv_form, window, cx);
            });
        })
}

pub(crate) fn password_row(
    label: &str,
    id: &'static str,
    secret: &Entity<InputState>,
    cx: &App,
) -> Field {
    let presence = secret_state_caption(secret.read(cx).value().as_ref()).to_string();
    field().label(label.to_string()).child(
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(id)
                    .test_support()
                    .child(Input::new(secret).mask_toggle()),
            )
            .child(
                div()
                    .id(format!("{id}-state"))
                    .test_support()
                    .text_sm()
                    .child(presence),
            ),
    )
}

pub(crate) fn advanced_toggle_button(
    label: &'static str,
    view: &Entity<AppView>,
    form: &ConfigForm,
) -> impl IntoElement {
    let toggle_view = view.clone();
    let toggle_cell = form.advanced.clone();
    let toggle_form = form.clone();
    Button::new("params-advanced-toggle")
        .small()
        .label(label)
        .on_click(move |_, window, cx| {
            toggle_view.update(cx, |_, cx| {
                let expanding = !*toggle_cell.borrow();
                if expanding {
                    toggle_form.expand_merge(window, cx);
                } else {
                    toggle_form.collapse_echo(window, cx);
                }
                *toggle_cell.borrow_mut() = expanding;
                cx.notify();
            });
        })
}

pub(crate) fn panel_toggle_button(
    id: &'static str,
    label: &'static str,
    view: &Entity<AppView>,
    open: &Rc<RefCell<bool>>,
) -> impl IntoElement {
    let toggle_view = view.clone();
    let toggle_cell = open.clone();
    Button::new(id)
        .small()
        .label(label)
        .on_click(move |_, _, cx| {
            toggle_view.update(cx, |_, cx| {
                let expanded = !*toggle_cell.borrow();
                *toggle_cell.borrow_mut() = expanded;
                cx.notify();
            });
        })
}

#[derive(Clone)]
pub(crate) struct ConfigForm {
    pub(crate) name: Entity<InputState>,

    pub(crate) protocol_select: Entity<SelectState<ProtocolChoiceList>>,

    pub(crate) last_protocol: Rc<RefCell<String>>,

    pub(crate) protocol_locked: bool,

    pub(crate) locked_protocol: String,

    pub(crate) vless_uuid: Entity<InputState>,
    pub(crate) vless_flow: Entity<SelectState<PanelChoiceList>>,
    pub(crate) vless_packet_encoding: Entity<SelectState<PanelChoiceList>>,

    pub(crate) s1_network: Entity<SelectState<PanelChoiceList>>,
    pub(crate) s1_path: Entity<InputState>,
    pub(crate) s1_host: Entity<InputState>,
    pub(crate) s1_method: Entity<InputState>,
    pub(crate) s1_service_name: Entity<InputState>,
    pub(crate) s1_open: Rc<RefCell<bool>>,

    pub(crate) s2_enable: Rc<RefCell<bool>>,
    pub(crate) s2_sni: Entity<InputState>,
    pub(crate) s2_alpn: Entity<InputState>,
    pub(crate) s2_fp: Entity<SelectState<PanelChoiceList>>,
    pub(crate) s2_insecure: Rc<RefCell<bool>>,
    pub(crate) s2_pbk: Entity<InputState>,
    pub(crate) s2_sid: Entity<InputState>,
    pub(crate) s2_open: Rc<RefCell<bool>>,

    pub(crate) s3_mode: Rc<RefCell<String>>,
    pub(crate) s3_brutal: Rc<RefCell<bool>>,
    pub(crate) s3_up: Entity<InputState>,
    pub(crate) s3_down: Entity<InputState>,
    pub(crate) s3_open: Rc<RefCell<bool>>,

    pub(crate) s4_reuse: Rc<RefCell<bool>>,
    pub(crate) s4_fastopen: Rc<RefCell<bool>>,
    pub(crate) s4_fragment: Rc<RefCell<bool>>,
    pub(crate) s4_multipath: Rc<RefCell<bool>>,
    pub(crate) s4_timeout: Entity<InputState>,
    pub(crate) s4_bind: Entity<InputState>,
    pub(crate) s4_inet4: Entity<InputState>,
    pub(crate) s4_inet6: Entity<InputState>,

    pub(crate) vless_open: Rc<RefCell<bool>>,

    pub(crate) vmess_uuid: Entity<InputState>,
    pub(crate) vmess_aid: Entity<InputState>,
    pub(crate) vmess_security: Entity<SelectState<PanelChoiceList>>,
    pub(crate) vmess_packet_encoding: Entity<SelectState<PanelChoiceList>>,

    pub(crate) vmess_open: Rc<RefCell<bool>>,

    pub(crate) trojan_password: Entity<InputState>,

    pub(crate) trojan_open: Rc<RefCell<bool>>,

    pub(crate) ss_method: Entity<SelectState<PanelChoiceList>>,
    pub(crate) ss_password: Entity<InputState>,
    pub(crate) ss_plugin: Entity<SelectState<PanelChoiceList>>,
    pub(crate) ss_plugin_opts: Entity<InputState>,
    pub(crate) ss_uot: Rc<RefCell<bool>>,

    pub(crate) ss_open: Rc<RefCell<bool>>,

    pub(crate) wg_private: Entity<InputState>,
    pub(crate) wg_public: Entity<InputState>,
    pub(crate) wg_preshared: Entity<InputState>,
    pub(crate) wg_reserved: Entity<InputState>,
    pub(crate) wg_keepalive: Entity<InputState>,
    pub(crate) wg_local_addr: Entity<InputState>,
    pub(crate) wg_dns: Entity<InputState>,
    pub(crate) wg_mtu: Entity<InputState>,
    pub(crate) wg_workers: Entity<InputState>,
    pub(crate) wg_sys_ifc: Rc<RefCell<bool>>,

    pub(crate) wg_open: Rc<RefCell<bool>>,

    pub(crate) awg_private: Entity<InputState>,
    pub(crate) awg_public: Entity<InputState>,
    pub(crate) awg_preshared: Entity<InputState>,
    pub(crate) awg_reserved: Entity<InputState>,
    pub(crate) awg_keepalive: Entity<InputState>,
    pub(crate) awg_local_addr: Entity<InputState>,
    pub(crate) awg_dns: Entity<InputState>,
    pub(crate) awg_mtu: Entity<InputState>,
    pub(crate) awg_workers: Entity<InputState>,
    pub(crate) awg_sys_ifc: Rc<RefCell<bool>>,
    pub(crate) awg_version: Entity<SelectState<PanelChoiceList>>,
    pub(crate) awg_enable: Rc<RefCell<bool>>,
    pub(crate) awg_jc: Entity<InputState>,
    pub(crate) awg_jmin: Entity<InputState>,
    pub(crate) awg_jmax: Entity<InputState>,
    pub(crate) awg_s1: Entity<InputState>,
    pub(crate) awg_s2: Entity<InputState>,
    pub(crate) awg_s3: Entity<InputState>,
    pub(crate) awg_s4: Entity<InputState>,
    pub(crate) awg_h1: Entity<InputState>,
    pub(crate) awg_h2: Entity<InputState>,
    pub(crate) awg_h3: Entity<InputState>,
    pub(crate) awg_h4: Entity<InputState>,
    pub(crate) awg_i1: Entity<InputState>,
    pub(crate) awg_i2: Entity<InputState>,
    pub(crate) awg_i3: Entity<InputState>,
    pub(crate) awg_i4: Entity<InputState>,
    pub(crate) awg_i5: Entity<InputState>,
    pub(crate) awg_hpk: Entity<InputState>,
    pub(crate) awg_pad: Entity<InputState>,
    pub(crate) awg_rekey_after: Entity<InputState>,
    pub(crate) awg_rekey_timeout: Entity<InputState>,
    pub(crate) awg_reject_after: Entity<InputState>,
    pub(crate) awg_keepalive_timeout: Entity<InputState>,
    pub(crate) awg_max_handshake: Entity<InputState>,
    pub(crate) awg_trailers: Rc<RefCell<bool>>,
    pub(crate) awg_cookies: Rc<RefCell<bool>>,

    pub(crate) awg_open: Rc<RefCell<bool>>,

    pub(crate) hy_server_ports: Entity<InputState>,
    pub(crate) hy_hop_interval: Entity<InputState>,
    pub(crate) hy_up: Entity<InputState>,
    pub(crate) hy_down: Entity<InputState>,
    pub(crate) hy_obfs: Entity<InputState>,
    pub(crate) hy_auth_type: Entity<SelectState<PanelChoiceList>>,
    pub(crate) hy_auth: Entity<InputState>,
    pub(crate) hy_recv_conn: Entity<InputState>,
    pub(crate) hy_recv: Entity<InputState>,
    pub(crate) hy_mtu: Rc<RefCell<bool>>,
    pub(crate) hy_password: Entity<InputState>,

    pub(crate) hy_open: Rc<RefCell<bool>>,

    pub(crate) tuic_uuid: Entity<InputState>,
    pub(crate) tuic_password: Entity<InputState>,
    pub(crate) tuic_cc: Entity<SelectState<PanelChoiceList>>,
    pub(crate) tuic_relay: Entity<SelectState<PanelChoiceList>>,
    pub(crate) tuic_uot: Rc<RefCell<bool>>,
    pub(crate) tuic_zero_rtt: Rc<RefCell<bool>>,
    pub(crate) tuic_heartbeat: Entity<InputState>,

    pub(crate) tuic_open: Rc<RefCell<bool>>,

    pub(crate) socks_version: Entity<SelectState<PanelChoiceList>>,
    pub(crate) socks_username: Entity<InputState>,
    pub(crate) socks_password: Entity<InputState>,
    pub(crate) socks_network: Entity<SelectState<PanelChoiceList>>,
    pub(crate) socks_uot: Rc<RefCell<bool>>,

    pub(crate) socks_open: Rc<RefCell<bool>>,

    pub(crate) http_username: Entity<InputState>,
    pub(crate) http_password: Entity<InputState>,
    pub(crate) http_path: Entity<InputState>,
    pub(crate) http_headers: Entity<InputState>,

    pub(crate) http_open: Rc<RefCell<bool>>,

    pub(crate) anytls_password: Entity<InputState>,
    pub(crate) anytls_interval: Entity<InputState>,
    pub(crate) anytls_timeout: Entity<InputState>,
    pub(crate) anytls_min: Entity<InputState>,

    pub(crate) anytls_open: Rc<RefCell<bool>>,

    pub(crate) naive_username: Entity<InputState>,
    pub(crate) naive_password: Entity<InputState>,
    pub(crate) naive_uot: Rc<RefCell<bool>>,
    pub(crate) naive_quic: Rc<RefCell<bool>>,
    pub(crate) naive_cc: Entity<SelectState<PanelChoiceList>>,

    pub(crate) naive_open: Rc<RefCell<bool>>,

    pub(crate) ssh_user: Entity<InputState>,
    pub(crate) ssh_password: Entity<InputState>,
    pub(crate) ssh_private_key: Entity<TextareaState>,
    pub(crate) ssh_key_path: Entity<InputState>,
    pub(crate) ssh_key_pass: Entity<InputState>,
    pub(crate) ssh_host_key: Entity<InputState>,
    pub(crate) ssh_host_key_algs: Entity<InputState>,
    pub(crate) ssh_client_version: Entity<InputState>,

    pub(crate) ssh_open: Rc<RefCell<bool>>,

    pub(crate) snell_psk: Entity<InputState>,
    pub(crate) snell_version: Entity<SelectState<PanelChoiceList>>,
    pub(crate) snell_obfs_mode: Entity<InputState>,
    pub(crate) snell_obfs_host: Entity<InputState>,
    pub(crate) snell_userkey: Entity<InputState>,
    pub(crate) snell_reuse: Rc<RefCell<bool>>,
    pub(crate) snell_network: Entity<InputState>,
    pub(crate) snell_mode: Entity<InputState>,

    pub(crate) snell_open: Rc<RefCell<bool>>,

    pub(crate) shadowtls_version: Entity<SelectState<PanelChoiceList>>,
    pub(crate) shadowtls_password: Entity<InputState>,

    pub(crate) shadowtls_open: Rc<RefCell<bool>>,

    pub(crate) xray_uuid: Entity<InputState>,
    pub(crate) xray_enc: Entity<InputState>,
    pub(crate) xray_flow: Entity<SelectState<PanelChoiceList>>,
    pub(crate) xray_sni: Entity<InputState>,
    pub(crate) xray_fp: Entity<SelectState<PanelChoiceList>>,
    pub(crate) xray_alpn: Entity<InputState>,
    pub(crate) xray_pbk: Entity<InputState>,
    pub(crate) xray_sid: Entity<InputState>,
    pub(crate) xray_spiderx: Entity<InputState>,
    pub(crate) xray_network: Entity<SelectState<PanelChoiceList>>,
    pub(crate) xray_mode: Entity<InputState>,
    pub(crate) xray_mux: Entity<SelectState<PanelChoiceList>>,

    pub(crate) xray_open: Rc<RefCell<bool>>,

    pub(crate) chain_hops: Rc<RefCell<Vec<serde_json::Value>>>,

    pub(crate) chain_pick: Entity<SelectState<PanelChoiceList>>,

    pub(crate) chain_library: Rc<RefCell<Vec<ChainLibraryEntry>>>,

    pub(crate) chain_open: Rc<RefCell<bool>>,

    pub(crate) extra_exe: Entity<InputState>,
    pub(crate) extra_args: Entity<InputState>,
    pub(crate) extra_config: Entity<TextareaState>,

    pub(crate) extra_open: Rc<RefCell<bool>>,

    pub(crate) params: Entity<TextareaState>,
    pub(crate) group: Entity<InputState>,
    pub(crate) bind: Entity<InputState>,
    pub(crate) port: Entity<InputState>,

    pub(crate) test_url: Entity<InputState>,

    pub(crate) guided_server: Entity<InputState>,
    pub(crate) guided_port: Entity<InputState>,
    pub(crate) guided_identity: Entity<InputState>,

    pub(crate) advanced: Rc<RefCell<bool>>,
}

impl ConfigForm {
    pub(crate) fn field(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: &str,
        default: Option<&str>,
    ) -> Entity<InputState> {
        cx.new(|cx| {
            let state = InputState::new(window, cx).placeholder(placeholder);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    pub(crate) fn secret_field(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: &str,
        default: Option<&str>,
    ) -> Entity<InputState> {
        cx.new(|cx| {
            let state = InputState::new(window, cx)
                .placeholder(placeholder)
                .masked(true);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    pub(crate) fn panel_select(
        window: &mut Window,
        cx: &mut Context<AppView>,
        choices: Vec<PanelChoice>,
        current: &str,
    ) -> Entity<SelectState<PanelChoiceList>> {
        let selected = choices
            .iter()
            .position(|choice| choice.value == current)
            .map(IndexPath::new);
        cx.new(|cx| SelectState::new(PanelChoiceList(choices), selected, window, cx))
    }

    pub(crate) fn area(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: &str,
        default: Option<&str>,
    ) -> Entity<TextareaState> {
        cx.new(|cx| {
            let state = TextareaState::new(window, cx).placeholder(placeholder);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    pub(crate) fn protocol_text(&self, cx: &App) -> String {
        if self.protocol_locked {
            return self.locked_protocol.clone();
        }
        self.protocol_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| self.last_protocol.borrow().clone())
    }

    pub(crate) fn sync_protocol_template(&self, window: &mut Window, cx: &mut App) {
        if self.protocol_locked {
            return;
        }
        let current = self
            .protocol_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| self.last_protocol.borrow().clone());
        let mut last = self.last_protocol.borrow_mut();
        if current == *last {
            return;
        }
        let old = last.clone();
        *last = current.clone();
        drop(last);
        let box_text = self.params.read(cx).value().to_string();
        let next = pristine_template_rewrite(&box_text, &old, &current);
        if next != box_text {
            self.params.update(cx, |state, cx| {
                state.set_value(next, window, cx);
            });
        }
    }

    pub(crate) fn blank(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let template = protocol_params_template(DEFAULT_PROTOCOL);
        let selected = protocol_choices()
            .iter()
            .position(|choice| choice.value == DEFAULT_PROTOCOL)
            .map(IndexPath::new);
        Self {
            name: Self::field(window, cx, "Name", None),
            protocol_select: cx.new(|cx| {
                SelectState::new(ProtocolChoiceList(protocol_choices()), selected, window, cx)
            }),
            last_protocol: Rc::new(RefCell::new(DEFAULT_PROTOCOL.to_string())),
            protocol_locked: false,
            locked_protocol: DEFAULT_PROTOCOL.to_string(),
            params: Self::area(
                window,
                cx,
                "Protocol params JSON (multiline)",
                Some(&template),
            ),
            group: Self::field(window, cx, "Group", Some(DEFAULT_GROUP)),
            bind: Self::field(window, cx, "Bind address", Some(DEFAULT_BIND)),
            port: Self::field(window, cx, "Port (blank or auto = auto-assign)", None),
            test_url: Self::field(window, cx, "Test URL (blank = global)", None),
            guided_server: Self::field(window, cx, "Server (e.g. example.com)", None),
            guided_port: Self::field(window, cx, "Port (e.g. 443)", None),
            guided_identity: Self::secret_field(window, cx, "Identity secret", None),

            vless_uuid: Self::secret_field(window, cx, "UUID", None),
            vless_flow: Self::panel_select(window, cx, flow_choices(), ""),
            vless_packet_encoding: Self::panel_select(window, cx, packet_encoding_choices(), ""),
            s1_network: Self::panel_select(window, cx, network_choices(), "tcp"),
            s1_path: Self::field(window, cx, "Path", None),
            s1_host: Self::field(window, cx, "Host", None),
            s1_method: Self::field(window, cx, "Method", None),
            s1_service_name: Self::field(window, cx, "Service name", None),
            s1_open: Rc::new(RefCell::new(false)),
            s2_enable: Rc::new(RefCell::new(false)),
            s2_sni: Self::field(window, cx, "SNI", None),
            s2_alpn: Self::field(window, cx, "ALPN (comma list)", None),
            s2_fp: Self::panel_select(window, cx, fp_choices(), ""),
            s2_insecure: Rc::new(RefCell::new(false)),
            s2_pbk: Self::field(window, cx, "Reality public key", None),
            s2_sid: Self::field(window, cx, "Reality short id", None),
            s2_open: Rc::new(RefCell::new(false)),
            s3_mode: Rc::new(RefCell::new(String::new())),
            s3_brutal: Rc::new(RefCell::new(false)),
            s3_up: Self::field(window, cx, "Brutal up Mbps", None),
            s3_down: Self::field(window, cx, "Brutal down Mbps", None),
            s3_open: Rc::new(RefCell::new(false)),
            s4_reuse: Rc::new(RefCell::new(false)),
            s4_fastopen: Rc::new(RefCell::new(false)),
            s4_fragment: Rc::new(RefCell::new(false)),
            s4_multipath: Rc::new(RefCell::new(false)),
            s4_timeout: Self::field(window, cx, "Connect timeout (e.g. 5s)", None),
            s4_bind: Self::field(window, cx, "Bind interface", None),
            s4_inet4: Self::field(window, cx, "IPv4 bind address", None),
            s4_inet6: Self::field(window, cx, "IPv6 bind address", None),
            vless_open: Rc::new(RefCell::new(false)),

            vmess_uuid: Self::secret_field(window, cx, "UUID", None),
            vmess_aid: Self::field(window, cx, "Alter ID (aid)", None),
            vmess_security: Self::panel_select(window, cx, vmess_security_choices(), "auto"),
            vmess_packet_encoding: Self::panel_select(window, cx, packet_encoding_choices(), ""),
            vmess_open: Rc::new(RefCell::new(false)),
            trojan_password: Self::secret_field(window, cx, "Password", None),
            trojan_open: Rc::new(RefCell::new(false)),
            ss_method: Self::panel_select(window, cx, ss_method_choices(), ""),
            ss_password: Self::secret_field(window, cx, "Password", None),
            ss_plugin: Self::panel_select(window, cx, ss_plugin_choices(), ""),
            ss_plugin_opts: Self::field(window, cx, "Plugin options", None),
            ss_uot: Rc::new(RefCell::new(false)),
            ss_open: Rc::new(RefCell::new(false)),

            wg_private: Self::secret_field(window, cx, "Private key", None),
            wg_public: Self::field(window, cx, "Peer public key", None),
            wg_preshared: Self::secret_field(window, cx, "Preshared key", None),
            wg_reserved: Self::field(window, cx, "Reserved (comma ints)", None),
            wg_keepalive: Self::field(window, cx, "Keepalive (seconds)", None),
            wg_local_addr: Self::field(window, cx, "Local address (comma CIDRs)", None),
            wg_dns: Self::field(window, cx, "DNS", None),
            wg_mtu: Self::field(window, cx, "MTU", None),
            wg_workers: Self::field(window, cx, "Workers", None),
            wg_sys_ifc: Rc::new(RefCell::new(false)),
            wg_open: Rc::new(RefCell::new(false)),
            awg_private: Self::secret_field(window, cx, "Private key", None),
            awg_public: Self::field(window, cx, "Peer public key", None),
            awg_preshared: Self::secret_field(window, cx, "Preshared key", None),
            awg_reserved: Self::field(window, cx, "Reserved (comma ints)", None),
            awg_keepalive: Self::field(window, cx, "Keepalive (seconds)", None),
            awg_local_addr: Self::field(window, cx, "Local address (comma CIDRs)", None),
            awg_dns: Self::field(window, cx, "DNS", None),
            awg_mtu: Self::field(window, cx, "MTU", None),
            awg_workers: Self::field(window, cx, "Workers", None),
            awg_sys_ifc: Rc::new(RefCell::new(false)),
            awg_version: Self::panel_select(window, cx, awg_version_choices(), ""),
            awg_enable: Rc::new(RefCell::new(false)),
            awg_jc: Self::field(window, cx, "Jc", None),
            awg_jmin: Self::field(window, cx, "Jmin", None),
            awg_jmax: Self::field(window, cx, "Jmax", None),
            awg_s1: Self::field(window, cx, "S1", None),
            awg_s2: Self::field(window, cx, "S2", None),
            awg_s3: Self::field(window, cx, "S3", None),
            awg_s4: Self::field(window, cx, "S4", None),
            awg_h1: Self::field(window, cx, "H1", None),
            awg_h2: Self::field(window, cx, "H2", None),
            awg_h3: Self::field(window, cx, "H3", None),
            awg_h4: Self::field(window, cx, "H4", None),
            awg_i1: Self::field(window, cx, "I1", None),
            awg_i2: Self::field(window, cx, "I2", None),
            awg_i3: Self::field(window, cx, "I3", None),
            awg_i4: Self::field(window, cx, "I4", None),
            awg_i5: Self::field(window, cx, "I5", None),
            awg_hpk: Self::field(window, cx, "Header protection key", None),
            awg_pad: Self::field(window, cx, "Content padding addition", None),
            awg_rekey_after: Self::field(window, cx, "Rekey after time", None),
            awg_rekey_timeout: Self::field(window, cx, "Rekey timeout", None),
            awg_reject_after: Self::field(window, cx, "Reject after time", None),
            awg_keepalive_timeout: Self::field(window, cx, "Keepalive timeout", None),
            awg_max_handshake: Self::field(window, cx, "Max handshake attempts", None),
            awg_trailers: Rc::new(RefCell::new(false)),
            awg_cookies: Rc::new(RefCell::new(false)),
            awg_open: Rc::new(RefCell::new(false)),

            hy_server_ports: Self::field(window, cx, "Server ports (e.g. 9000-9010)", None),
            hy_hop_interval: Self::field(window, cx, "Hop interval (e.g. 30s)", None),
            hy_up: Self::field(window, cx, "Up Mbps", None),
            hy_down: Self::field(window, cx, "Down Mbps", None),
            hy_obfs: Self::field(window, cx, "Obfs", None),
            hy_auth_type: Self::panel_select(window, cx, hy_auth_type_choices(), ""),
            hy_auth: Self::secret_field(window, cx, "Auth secret", None),
            hy_recv_conn: Self::field(window, cx, "Recv window conn", None),
            hy_recv: Self::field(window, cx, "Recv window", None),
            hy_mtu: Rc::new(RefCell::new(false)),
            hy_password: Self::secret_field(window, cx, "Password", None),
            hy_open: Rc::new(RefCell::new(false)),
            tuic_uuid: Self::secret_field(window, cx, "UUID", None),
            tuic_password: Self::secret_field(window, cx, "Password", None),
            tuic_cc: Self::panel_select(window, cx, tuic_cc_choices(), ""),
            tuic_relay: Self::panel_select(window, cx, tuic_relay_choices(), ""),
            tuic_uot: Rc::new(RefCell::new(false)),
            tuic_zero_rtt: Rc::new(RefCell::new(false)),
            tuic_heartbeat: Self::field(window, cx, "Heartbeat (e.g. 10s)", None),
            tuic_open: Rc::new(RefCell::new(false)),

            socks_version: Self::panel_select(window, cx, socks_version_choices(), ""),
            socks_username: Self::field(window, cx, "Username", None),
            socks_password: Self::secret_field(window, cx, "Password", None),
            socks_network: Self::panel_select(window, cx, socks_network_choices(), "tcp"),
            socks_uot: Rc::new(RefCell::new(false)),
            socks_open: Rc::new(RefCell::new(false)),
            http_username: Self::field(window, cx, "Username", None),
            http_password: Self::secret_field(window, cx, "Password", None),
            http_path: Self::field(window, cx, "Path", None),
            http_headers: Self::field(window, cx, "Headers", None),
            http_open: Rc::new(RefCell::new(false)),
            anytls_password: Self::secret_field(window, cx, "Password", None),
            anytls_interval: Self::field(window, cx, "Idle check interval (e.g. 30s)", None),
            anytls_timeout: Self::field(window, cx, "Idle timeout (e.g. 30s)", None),
            anytls_min: Self::field(window, cx, "Min idle sessions", None),
            anytls_open: Rc::new(RefCell::new(false)),
            naive_username: Self::field(window, cx, "Username", None),
            naive_password: Self::secret_field(window, cx, "Password", None),
            naive_uot: Rc::new(RefCell::new(false)),
            naive_quic: Rc::new(RefCell::new(false)),
            naive_cc: Self::panel_select(window, cx, naive_cc_choices(), ""),
            naive_open: Rc::new(RefCell::new(false)),
            ssh_user: Self::field(window, cx, "User", None),
            ssh_password: Self::secret_field(window, cx, "Password", None),
            ssh_private_key: Self::area(window, cx, "Private key (paste or Browse…)", None),
            ssh_key_path: Self::field(window, cx, "Private key path", None),
            ssh_key_pass: Self::secret_field(window, cx, "Private key passphrase", None),
            ssh_host_key: Self::field(window, cx, "Host key (comma list)", None),
            ssh_host_key_algs: Self::field(window, cx, "Host key algorithms (comma list)", None),
            ssh_client_version: Self::field(window, cx, "Client version", None),
            ssh_open: Rc::new(RefCell::new(false)),
            snell_psk: Self::secret_field(window, cx, "PSK", None),
            snell_version: Self::panel_select(window, cx, snell_version_choices(), ""),
            snell_obfs_mode: Self::field(window, cx, "Obfs mode", None),
            snell_obfs_host: Self::field(window, cx, "Obfs host", None),
            snell_userkey: Self::field(window, cx, "User key", None),
            snell_reuse: Rc::new(RefCell::new(false)),
            snell_network: Self::field(window, cx, "Network", None),
            snell_mode: Self::field(window, cx, "Mode", None),
            snell_open: Rc::new(RefCell::new(false)),
            shadowtls_version: Self::panel_select(window, cx, shadowtls_version_choices(), ""),
            shadowtls_password: Self::secret_field(window, cx, "Password", None),
            shadowtls_open: Rc::new(RefCell::new(false)),
            xray_uuid: Self::secret_field(window, cx, "UUID", None),
            xray_enc: Self::field(window, cx, "Encryption (default none)", None),
            xray_flow: Self::panel_select(window, cx, xray_flow_choices(), ""),
            xray_sni: Self::field(window, cx, "SNI", None),
            xray_fp: Self::panel_select(window, cx, fp_choices(), ""),
            xray_alpn: Self::field(window, cx, "ALPN (comma list)", None),
            xray_pbk: Self::field(window, cx, "REALITY public key (pbk)", None),
            xray_sid: Self::field(window, cx, "REALITY short id (sid)", None),
            xray_spiderx: Self::field(window, cx, "REALITY spiderX path", None),
            xray_network: Self::panel_select(window, cx, xray_network_choices(), ""),
            xray_mode: Self::field(window, cx, "Mode", None),
            xray_mux: Self::panel_select(window, cx, xray_mux_choices(), ""),
            xray_open: Rc::new(RefCell::new(false)),
            chain_hops: Rc::new(RefCell::new(Vec::new())),
            chain_pick: Self::panel_select(window, cx, chain_pick_choices(&[]), ""),
            chain_library: Rc::new(RefCell::new(Vec::new())),
            chain_open: Rc::new(RefCell::new(false)),
            extra_exe: Self::field(window, cx, "Executable (app dir file name)", None),
            extra_args: Self::field(window, cx, "Args", None),
            extra_config: Self::area(window, cx, "Config JSON (passed as -c <file>)", None),
            extra_open: Rc::new(RefCell::new(false)),
            advanced: Rc::new(RefCell::new(false)),
        }
    }

    pub(crate) fn prefilled(
        window: &mut Window,
        cx: &mut Context<AppView>,
        config: &ProxyConfig,
        test_url: Option<&str>,
    ) -> Self {
        let read_keys: &[&str] = guided_identity_fields(&config.protocol)
            .map(|guided| guided.read_keys)
            .unwrap_or(&[]);
        let selected = protocol_choices()
            .iter()
            .position(|choice| choice.value == config.protocol)
            .map(IndexPath::new);

        let params = config.params.as_str();
        let network_value = guided_prefill_value(params, &["network"]);
        let network = if network_value.is_empty() {
            "tcp".to_string()
        } else {
            network_value
        };
        let security = guided_prefill_value(params, &["security"]);

        let tls_on = if config.protocol == "vmess" {
            vmess_tls_enabled(params)
        } else {
            matches!(
                security.to_ascii_lowercase().as_str(),
                "tls" | "xtls" | "reality"
            )
        };
        Self {
            name: Self::field(window, cx, "Name", Some(config.name.as_str())),
            protocol_select: cx.new(|cx| {
                SelectState::new(ProtocolChoiceList(protocol_choices()), selected, window, cx)
            }),
            last_protocol: Rc::new(RefCell::new(config.protocol.clone())),
            protocol_locked: true,
            locked_protocol: config.protocol.clone(),
            params: Self::area(
                window,
                cx,
                "Protocol params JSON (multiline)",
                Some(config.params.as_str()),
            ),
            group: Self::field(window, cx, "Group", Some(config.group.as_str())),
            bind: Self::field(window, cx, "Bind address", Some(config.bind_ip.as_str())),
            port: Self::field(
                window,
                cx,
                "Port (blank or auto = auto-assign)",
                Some(&config.port.to_string()),
            ),
            test_url: Self::field(window, cx, "Test URL (blank = global)", test_url),
            guided_server: Self::field(
                window,
                cx,
                "Server (e.g. example.com)",
                Some(&guided_prefill_value(
                    &config.params,
                    &["server", "address"],
                )),
            ),
            guided_port: Self::field(
                window,
                cx,
                "Port (e.g. 443)",
                Some(&guided_prefill_value(
                    &config.params,
                    &["server_port", "port"],
                )),
            ),
            guided_identity: Self::secret_field(
                window,
                cx,
                "Identity secret",
                Some(&guided_prefill_value(&config.params, read_keys)),
            ),
            vless_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            vless_flow: Self::panel_select(
                window,
                cx,
                flow_choices(),
                &guided_prefill_value(params, &["flow"]),
            ),
            vless_packet_encoding: Self::panel_select(
                window,
                cx,
                packet_encoding_choices(),
                &guided_prefill_value(params, &["packet_encoding", "packetEncoding"]),
            ),
            s1_network: Self::panel_select(window, cx, network_choices(), &network),
            s1_path: Self::field(
                window,
                cx,
                "Path",
                Some(&guided_prefill_value(params, &["path"])),
            ),
            s1_host: Self::field(
                window,
                cx,
                "Host",
                Some(&guided_prefill_value(params, &["host"])),
            ),
            s1_method: Self::field(
                window,
                cx,
                "Method",
                Some(&guided_prefill_value(params, &["method"])),
            ),
            s1_service_name: Self::field(
                window,
                cx,
                "Service name",
                Some(&guided_prefill_value(
                    params,
                    &["service_name", "serviceName"],
                )),
            ),
            s1_open: Rc::new(RefCell::new(false)),
            s2_enable: Rc::new(RefCell::new(tls_on)),
            s2_sni: Self::field(
                window,
                cx,
                "SNI",
                Some(&guided_prefill_value(params, &["sni", "peer"])),
            ),
            s2_alpn: Self::field(
                window,
                cx,
                "ALPN (comma list)",
                Some(&guided_prefill_value(params, &["alpn"])),
            ),
            s2_fp: Self::panel_select(
                window,
                cx,
                fp_choices(),
                &guided_prefill_value(params, &["fp", "fingerprint"]),
            ),
            s2_insecure: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &[
                    "insecure",
                    "allowInsecure",
                    "allow_insecure",
                    "allowinsecure",
                ],
            ))),
            s2_pbk: Self::field(
                window,
                cx,
                "Reality public key",
                Some(&guided_prefill_value(params, &["pbk"])),
            ),
            s2_sid: Self::field(
                window,
                cx,
                "Reality short id",
                Some(&guided_prefill_value(params, &["sid"])),
            ),
            s2_open: Rc::new(RefCell::new(false)),
            s3_mode: Rc::new(RefCell::new(guided_prefill_value(params, &["mux"]))),
            s3_brutal: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["brutal_enable"],
            ))),
            s3_up: Self::field(
                window,
                cx,
                "Brutal up Mbps",
                Some(&guided_prefill_value(
                    params,
                    &["brutal_up_mbps", "brutal_up"],
                )),
            ),
            s3_down: Self::field(
                window,
                cx,
                "Brutal down Mbps",
                Some(&guided_prefill_value(
                    params,
                    &["brutal_down_mbps", "brutal_down"],
                )),
            ),
            s3_open: Rc::new(RefCell::new(false)),
            s4_reuse: Rc::new(RefCell::new(guided_prefill_bool(params, &["reuse_addr"]))),
            s4_fastopen: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["tcp_fast_open"],
            ))),
            s4_fragment: Rc::new(RefCell::new(guided_prefill_bool(params, &["udp_fragment"]))),
            s4_multipath: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["tcp_multi_path"],
            ))),
            s4_timeout: Self::field(
                window,
                cx,
                "Connect timeout (e.g. 5s)",
                Some(&guided_prefill_value(params, &["connect_timeout"])),
            ),
            s4_bind: Self::field(
                window,
                cx,
                "Bind interface",
                Some(&guided_prefill_value(params, &["bind_interface"])),
            ),
            s4_inet4: Self::field(
                window,
                cx,
                "IPv4 bind address",
                Some(&guided_prefill_value(params, &["inet4_bind_address"])),
            ),
            s4_inet6: Self::field(
                window,
                cx,
                "IPv6 bind address",
                Some(&guided_prefill_value(params, &["inet6_bind_address"])),
            ),
            vless_open: Rc::new(RefCell::new(false)),

            vmess_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            vmess_aid: Self::field(
                window,
                cx,
                "Alter ID (aid)",
                Some(&guided_prefill_value(params, &["alter_id", "aid"])),
            ),
            vmess_security: Self::panel_select(
                window,
                cx,
                vmess_security_choices(),
                &vmess_cipher_value(params),
            ),
            vmess_packet_encoding: Self::panel_select(
                window,
                cx,
                packet_encoding_choices(),
                &guided_prefill_value(params, &["packet_encoding", "packetEncoding"]),
            ),
            vmess_open: Rc::new(RefCell::new(false)),
            trojan_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            trojan_open: Rc::new(RefCell::new(false)),
            ss_method: Self::panel_select(
                window,
                cx,
                ss_method_choices(),
                &guided_prefill_value(params, &["method"]),
            ),
            ss_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            ss_plugin: Self::panel_select(
                window,
                cx,
                ss_plugin_choices(),
                &guided_prefill_value(params, &["plugin"]),
            ),
            ss_plugin_opts: Self::field(
                window,
                cx,
                "Plugin options",
                Some(&guided_prefill_value(
                    params,
                    &["plugin_opts", "plugin-opts", "pluginOpts"],
                )),
            ),
            ss_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),
            ss_open: Rc::new(RefCell::new(false)),

            wg_private: Self::secret_field(
                window,
                cx,
                "Private key",
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            wg_public: Self::field(
                window,
                cx,
                "Peer public key",
                Some(&guided_prefill_value(params, &["public_key", "publickey"])),
            ),
            wg_preshared: Self::secret_field(
                window,
                cx,
                "Preshared key",
                Some(&guided_prefill_value(
                    params,
                    &["pre_shared_key", "presharedkey", "preshared_key"],
                )),
            ),
            wg_reserved: Self::field(
                window,
                cx,
                "Reserved (comma ints)",
                Some(&guided_prefill_value(params, &["reserved"])),
            ),
            wg_keepalive: Self::field(
                window,
                cx,
                "Keepalive (seconds)",
                Some(&guided_prefill_value(
                    params,
                    &["persistent_keepalive", "persistent_keepalive_interval"],
                )),
            ),
            wg_local_addr: Self::field(
                window,
                cx,
                "Local address (comma CIDRs)",
                Some(&guided_prefill_value(
                    params,
                    &["address", "local_address", "local_addr"],
                )),
            ),
            wg_dns: Self::field(
                window,
                cx,
                "DNS",
                Some(&guided_prefill_value(params, &["dns"])),
            ),
            wg_mtu: Self::field(
                window,
                cx,
                "MTU",
                Some(&guided_prefill_value(params, &["mtu"])),
            ),
            wg_workers: Self::field(
                window,
                cx,
                "Workers",
                Some(&guided_prefill_value(params, &["workers"])),
            ),
            wg_sys_ifc: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["system", "sys_ifc"],
            ))),
            wg_open: Rc::new(RefCell::new(false)),
            awg_private: Self::secret_field(
                window,
                cx,
                "Private key",
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            awg_public: Self::field(
                window,
                cx,
                "Peer public key",
                Some(&guided_prefill_value(params, &["public_key", "publickey"])),
            ),
            awg_preshared: Self::secret_field(
                window,
                cx,
                "Preshared key",
                Some(&guided_prefill_value(
                    params,
                    &["pre_shared_key", "presharedkey", "preshared_key"],
                )),
            ),
            awg_reserved: Self::field(
                window,
                cx,
                "Reserved (comma ints)",
                Some(&guided_prefill_value(params, &["reserved"])),
            ),
            awg_keepalive: Self::field(
                window,
                cx,
                "Keepalive (seconds)",
                Some(&guided_prefill_value(
                    params,
                    &["persistent_keepalive", "persistent_keepalive_interval"],
                )),
            ),
            awg_local_addr: Self::field(
                window,
                cx,
                "Local address (comma CIDRs)",
                Some(&guided_prefill_value(
                    params,
                    &["address", "local_address", "local_addr"],
                )),
            ),
            awg_dns: Self::field(
                window,
                cx,
                "DNS",
                Some(&guided_prefill_value(params, &["dns"])),
            ),
            awg_mtu: Self::field(
                window,
                cx,
                "MTU",
                Some(&guided_prefill_value(params, &["mtu"])),
            ),
            awg_workers: Self::field(
                window,
                cx,
                "Workers",
                Some(&guided_prefill_value(params, &["workers"])),
            ),
            awg_sys_ifc: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["system", "sys_ifc"],
            ))),
            awg_version: Self::panel_select(
                window,
                cx,
                awg_version_choices(),
                &guided_prefill_value(params, &["awg_version", "version"]),
            ),
            awg_enable: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["enable_amnezia"],
            ))),
            awg_jc: Self::field(
                window,
                cx,
                "Jc",
                Some(&guided_prefill_value(params, &["jc"])),
            ),
            awg_jmin: Self::field(
                window,
                cx,
                "Jmin",
                Some(&guided_prefill_value(params, &["jmin"])),
            ),
            awg_jmax: Self::field(
                window,
                cx,
                "Jmax",
                Some(&guided_prefill_value(params, &["jmax"])),
            ),
            awg_s1: Self::field(
                window,
                cx,
                "S1",
                Some(&guided_prefill_value(params, &["s1"])),
            ),
            awg_s2: Self::field(
                window,
                cx,
                "S2",
                Some(&guided_prefill_value(params, &["s2"])),
            ),
            awg_s3: Self::field(
                window,
                cx,
                "S3",
                Some(&guided_prefill_value(params, &["s3"])),
            ),
            awg_s4: Self::field(
                window,
                cx,
                "S4",
                Some(&guided_prefill_value(params, &["s4"])),
            ),
            awg_h1: Self::field(
                window,
                cx,
                "H1",
                Some(&guided_prefill_value(params, &["h1"])),
            ),
            awg_h2: Self::field(
                window,
                cx,
                "H2",
                Some(&guided_prefill_value(params, &["h2"])),
            ),
            awg_h3: Self::field(
                window,
                cx,
                "H3",
                Some(&guided_prefill_value(params, &["h3"])),
            ),
            awg_h4: Self::field(
                window,
                cx,
                "H4",
                Some(&guided_prefill_value(params, &["h4"])),
            ),
            awg_i1: Self::field(
                window,
                cx,
                "I1",
                Some(&guided_prefill_value(params, &["i1"])),
            ),
            awg_i2: Self::field(
                window,
                cx,
                "I2",
                Some(&guided_prefill_value(params, &["i2"])),
            ),
            awg_i3: Self::field(
                window,
                cx,
                "I3",
                Some(&guided_prefill_value(params, &["i3"])),
            ),
            awg_i4: Self::field(
                window,
                cx,
                "I4",
                Some(&guided_prefill_value(params, &["i4"])),
            ),
            awg_i5: Self::field(
                window,
                cx,
                "I5",
                Some(&guided_prefill_value(params, &["i5"])),
            ),
            awg_hpk: Self::field(
                window,
                cx,
                "Header protection key",
                Some(&guided_prefill_value(params, &["headerprotectionkey"])),
            ),
            awg_pad: Self::field(
                window,
                cx,
                "Content padding addition",
                Some(&guided_prefill_value(params, &["contentpaddingaddition"])),
            ),
            awg_rekey_after: Self::field(
                window,
                cx,
                "Rekey after time",
                Some(&guided_prefill_value(params, &["rekeyaftertime"])),
            ),
            awg_rekey_timeout: Self::field(
                window,
                cx,
                "Rekey timeout",
                Some(&guided_prefill_value(params, &["rekeytimeout"])),
            ),
            awg_reject_after: Self::field(
                window,
                cx,
                "Reject after time",
                Some(&guided_prefill_value(params, &["rejectaftertime"])),
            ),
            awg_keepalive_timeout: Self::field(
                window,
                cx,
                "Keepalive timeout",
                Some(&guided_prefill_value(params, &["keepalivetimeout"])),
            ),
            awg_max_handshake: Self::field(
                window,
                cx,
                "Max handshake attempts",
                Some(&guided_prefill_value(params, &["maxhandshakeattempts"])),
            ),
            awg_trailers: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["randomtrailers", "random_trailers"],
            ))),
            awg_cookies: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["disablecookies", "disable_cookies"],
            ))),
            awg_open: Rc::new(RefCell::new(false)),

            hy_server_ports: Self::field(
                window,
                cx,
                "Server ports (e.g. 9000-9010)",
                Some(&guided_prefill_value(params, &["server_ports"])),
            ),
            hy_hop_interval: Self::field(
                window,
                cx,
                "Hop interval (e.g. 30s)",
                Some(&guided_prefill_value(params, &["hop_interval"])),
            ),
            hy_up: Self::field(
                window,
                cx,
                "Up Mbps",
                Some(&guided_prefill_value(params, &["up", "up_mbps", "upmbps"])),
            ),
            hy_down: Self::field(
                window,
                cx,
                "Down Mbps",
                Some(&guided_prefill_value(
                    params,
                    &["down", "down_mbps", "downmbps"],
                )),
            ),
            hy_obfs: Self::field(
                window,
                cx,
                "Obfs",
                Some(&guided_prefill_value(params, &["obfs"])),
            ),
            hy_auth_type: Self::panel_select(
                window,
                cx,
                hy_auth_type_choices(),
                &guided_prefill_value(params, &["auth_type"]),
            ),
            hy_auth: Self::secret_field(
                window,
                cx,
                "Auth secret",
                Some(&guided_prefill_value(params, &["auth"])),
            ),
            hy_recv_conn: Self::field(
                window,
                cx,
                "Recv window conn",
                Some(&guided_prefill_value(params, &["recv_window_conn"])),
            ),
            hy_recv: Self::field(
                window,
                cx,
                "Recv window",
                Some(&guided_prefill_value(params, &["recv_window"])),
            ),
            hy_mtu: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["disable_mtu_discovery"],
            ))),
            hy_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            hy_open: Rc::new(RefCell::new(false)),
            tuic_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            tuic_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            tuic_cc: Self::panel_select(
                window,
                cx,
                tuic_cc_choices(),
                &guided_prefill_value(params, &["congestion_control"]),
            ),
            tuic_relay: Self::panel_select(
                window,
                cx,
                tuic_relay_choices(),
                &guided_prefill_value(params, &["udp_relay_mode"]),
            ),
            tuic_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_stream"],
            ))),
            tuic_zero_rtt: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["zero_rtt_handshake", "zero_rtt"],
            ))),
            tuic_heartbeat: Self::field(
                window,
                cx,
                "Heartbeat (e.g. 10s)",
                Some(&guided_prefill_value(params, &["heartbeat"])),
            ),
            tuic_open: Rc::new(RefCell::new(false)),

            socks_version: Self::panel_select(
                window,
                cx,
                socks_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            socks_username: Self::field(
                window,
                cx,
                "Username",
                Some(&guided_prefill_value(params, &["username"])),
            ),
            socks_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            socks_network: Self::panel_select(
                window,
                cx,
                socks_network_choices(),
                &guided_prefill_value(params, &["network"]),
            ),
            socks_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),
            socks_open: Rc::new(RefCell::new(false)),
            http_username: Self::field(
                window,
                cx,
                "Username",
                Some(&guided_prefill_value(params, &["username"])),
            ),
            http_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            http_path: Self::field(
                window,
                cx,
                "Path",
                Some(&guided_prefill_value(params, &["path"])),
            ),
            http_headers: Self::field(
                window,
                cx,
                "Headers",
                Some(&guided_prefill_value(params, &["headers"])),
            ),
            http_open: Rc::new(RefCell::new(false)),
            anytls_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            anytls_interval: Self::field(
                window,
                cx,
                "Idle check interval (e.g. 30s)",
                Some(&guided_prefill_value(
                    params,
                    &["idle_session_check_interval", "interval"],
                )),
            ),
            anytls_timeout: Self::field(
                window,
                cx,
                "Idle timeout (e.g. 30s)",
                Some(&guided_prefill_value(
                    params,
                    &["idle_session_timeout", "timeout"],
                )),
            ),
            anytls_min: Self::field(
                window,
                cx,
                "Min idle sessions",
                Some(&guided_prefill_value(params, &["min_idle_session", "min"])),
            ),
            anytls_open: Rc::new(RefCell::new(false)),
            naive_username: Self::field(
                window,
                cx,
                "Username",
                Some(&guided_prefill_value(params, &["username"])),
            ),
            naive_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            naive_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),
            naive_quic: Rc::new(RefCell::new(guided_prefill_bool(params, &["quic"]))),
            naive_cc: Self::panel_select(
                window,
                cx,
                naive_cc_choices(),
                &guided_prefill_value(params, &["quic_congestion_control", "congestion_control"]),
            ),
            naive_open: Rc::new(RefCell::new(false)),
            ssh_user: Self::field(
                window,
                cx,
                "User",
                Some(&guided_prefill_value(params, &["user", "username"])),
            ),
            ssh_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            ssh_private_key: Self::area(
                window,
                cx,
                "Private key (paste or Browse…)",
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            ssh_key_path: Self::field(
                window,
                cx,
                "Private key path",
                Some(&guided_prefill_value(params, &["private_key_path"])),
            ),
            ssh_key_pass: Self::secret_field(
                window,
                cx,
                "Private key passphrase",
                Some(&guided_prefill_value(
                    params,
                    &["private_key_passphrase", "private_key_pass"],
                )),
            ),
            ssh_host_key: Self::field(
                window,
                cx,
                "Host key (comma list)",
                Some(&guided_prefill_list(params, &["host_key"])),
            ),
            ssh_host_key_algs: Self::field(
                window,
                cx,
                "Host key algorithms (comma list)",
                Some(&guided_prefill_list(params, &["host_key_algorithms"])),
            ),
            ssh_client_version: Self::field(
                window,
                cx,
                "Client version",
                Some(&guided_prefill_value(params, &["client_version"])),
            ),
            ssh_open: Rc::new(RefCell::new(false)),
            snell_psk: Self::secret_field(
                window,
                cx,
                "PSK",
                Some(&guided_prefill_value(params, &["psk"])),
            ),
            snell_version: Self::panel_select(
                window,
                cx,
                snell_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            snell_obfs_mode: Self::field(
                window,
                cx,
                "Obfs mode",
                Some(&guided_prefill_value(params, &["obfs_mode", "obfs"])),
            ),
            snell_obfs_host: Self::field(
                window,
                cx,
                "Obfs host",
                Some(&guided_prefill_value(params, &["obfs_host", "obfs-host"])),
            ),
            snell_userkey: Self::field(
                window,
                cx,
                "User key",
                Some(&guided_prefill_value(params, &["userkey"])),
            ),
            snell_reuse: Rc::new(RefCell::new(guided_prefill_bool(params, &["reuse"]))),
            snell_network: Self::field(
                window,
                cx,
                "Network",
                Some(&guided_prefill_value(params, &["network"])),
            ),
            snell_mode: Self::field(
                window,
                cx,
                "Mode",
                Some(&guided_prefill_value(params, &["mode"])),
            ),
            snell_open: Rc::new(RefCell::new(false)),
            shadowtls_version: Self::panel_select(
                window,
                cx,
                shadowtls_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            shadowtls_password: Self::secret_field(
                window,
                cx,
                "Password",
                Some(&guided_prefill_value(params, &["password"])),
            ),
            shadowtls_open: Rc::new(RefCell::new(false)),
            xray_uuid: Self::secret_field(
                window,
                cx,
                "UUID",
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            xray_enc: Self::field(
                window,
                cx,
                "Encryption (default none)",
                Some(&guided_prefill_value(params, &["encryption"])),
            ),
            xray_flow: Self::panel_select(
                window,
                cx,
                xray_flow_choices(),
                &guided_prefill_value(params, &["flow"]),
            ),
            xray_sni: Self::field(
                window,
                cx,
                "SNI",
                Some(&guided_prefill_value(params, &["sni"])),
            ),
            xray_fp: Self::panel_select(
                window,
                cx,
                fp_choices(),
                &guided_prefill_value(params, &["fp", "fingerprint"]),
            ),
            xray_alpn: Self::field(
                window,
                cx,
                "ALPN (comma list)",
                Some(&guided_prefill_value(params, &["alpn"])),
            ),
            xray_pbk: Self::field(
                window,
                cx,
                "REALITY public key (pbk)",
                Some(&guided_prefill_value(params, &["pbk"])),
            ),
            xray_sid: Self::field(
                window,
                cx,
                "REALITY short id (sid)",
                Some(&guided_prefill_value(params, &["sid"])),
            ),
            xray_spiderx: Self::field(
                window,
                cx,
                "REALITY spiderX path",
                Some(&guided_prefill_value(params, &["spiderx"])),
            ),
            xray_network: Self::panel_select(
                window,
                cx,
                xray_network_choices(),
                &guided_prefill_value(params, &["network"]),
            ),
            xray_mode: Self::field(
                window,
                cx,
                "Mode",
                Some(&guided_prefill_value(params, &["mode"])),
            ),
            xray_mux: Self::panel_select(
                window,
                cx,
                xray_mux_choices(),
                &guided_prefill_value(params, &["xray_mux"]),
            ),
            xray_open: Rc::new(RefCell::new(false)),
            chain_hops: Rc::new(RefCell::new(chain_nodes_of(params))),
            chain_pick: Self::panel_select(window, cx, chain_pick_choices(&[]), ""),
            chain_library: Rc::new(RefCell::new(Vec::new())),
            chain_open: Rc::new(RefCell::new(false)),
            extra_exe: Self::field(
                window,
                cx,
                "Executable (app dir file name)",
                Some(&guided_prefill_value(params, &["exe", "binary"])),
            ),
            extra_args: Self::field(
                window,
                cx,
                "Args",
                Some(&guided_prefill_value(params, &["args"])),
            ),
            extra_config: Self::area(
                window,
                cx,
                "Config JSON (passed as -c <file>)",
                Some(&extra_config_prefill(params)),
            ),
            extra_open: Rc::new(RefCell::new(false)),
            advanced: Rc::new(RefCell::new(true)),
        }
    }

    pub(crate) fn port_budget_text(rows: &[ProxyConfig]) -> String {
        use crate::store::{
            AUTO_PORT_END, AUTO_PORT_SPILL_END, AUTO_PORT_SPILL_START, AUTO_PORT_START,
        };
        let total = (AUTO_PORT_END - AUTO_PORT_START + 1) as usize
            + (AUTO_PORT_SPILL_END - AUTO_PORT_SPILL_START + 1) as usize;
        let used = rows
            .iter()
            .filter(|row| crate::store::is_auto_port(row.port))
            .count();
        format!("Auto ports: {used} of {total} in use (10801–10999, spill 18081–18380)")
    }

    pub(crate) fn uuid_row(
        &self,
        view: &Entity<AppView>,
        secret: &Entity<InputState>,
        id_prefix: &str,
        cx: &App,
    ) -> impl IntoElement {
        let secret_entity = secret.clone();
        let uuid_view = view.clone();
        let presence = secret_state_caption(secret.read(cx).value().as_ref()).to_string();
        field().label("UUID *".to_string()).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id(format!("{id_prefix}-uuid"))
                                .test_support()
                                .child(Input::new(secret).mask_toggle()),
                        )
                        .child(
                            Button::new(format!("{id_prefix}-uuid-generate"))
                                .small()
                                .label("Generate")
                                .tooltip("Generate a random UUIDv4")
                                .accessibility_label("Generate a random UUIDv4")
                                .on_click(move |_, window, cx| {
                                    let generated = generate_uuid_v4();
                                    uuid_view.update(cx, |_, cx| {
                                        secret_entity.update(cx, |state, cx| {
                                            state.set_value(generated, &mut *window, cx);
                                        });
                                    });
                                    window.push_notification(
                                        Notification::info("Generated a new UUID"),
                                        cx,
                                    );
                                }),
                        ),
                )
                .child(
                    div()
                        .id(format!("{id_prefix}-uuid-state"))
                        .test_support()
                        .text_sm()
                        .child(presence),
                ),
        )
    }

    pub(crate) fn panel_select_row(
        label: &str,
        tip_id: &'static str,
        tip: &str,
        select_id: &'static str,
        a11y: &str,
        state: &Entity<SelectState<PanelChoiceList>>,
    ) -> Field {
        let tip = tip.to_string();
        field().label(label.to_string()).child(
            div()
                .id(tip_id)
                .test_support()
                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
                .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                })
                .child(
                    Select::new(state)
                        .id(select_id)
                        .small()
                        .accessibility_label(a11y),
                ),
        )
    }

    pub(crate) fn page_disclosure(
        caption: &str,
        caption_id: &'static str,
        toggle_id: &'static str,
        section_id: &'static str,
        open: &Rc<RefCell<bool>>,
        view: &Entity<AppView>,
        block: impl IntoElement,
    ) -> impl IntoElement {
        Collapsible::new()
            .open(*open.borrow())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .id(caption_id)
                            .test_support()
                            .child(Label::new(caption.to_string())),
                    )
                    .child(panel_toggle_button(
                        toggle_id,
                        if *open.borrow() { "Hide" } else { "Show" },
                        view,
                        open,
                    )),
            )
            .content(div().id(section_id).test_support().child(block))
    }

    pub(crate) fn s1_network_section(&self, view: &Entity<AppView>, _cx: &App) -> impl IntoElement {
        let network_select = self.s1_network.clone();
        let picker = field().label("Network".to_string()).child(
            div()
                .id("s1-network-tip")
                .test_support()
                .tooltip(move |_, cx| {
                    cx.new(|_| Tooltip::new("Stream transport network".to_string()))
                        .into()
                })
                .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                })
                .child(
                    Select::new(&network_select)
                        .id("s1-network")
                        .small()
                        .accessibility_label("Stream transport network"),
                ),
        );
        field().label_indent(false).child(
            Collapsible::new()
                .open(*self.s1_open.borrow())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id("s1-caption")
                                .test_support()
                                .child(Label::new("Network")),
                        )
                        .child(panel_toggle_button(
                            "s1-toggle",
                            if *self.s1_open.borrow() {
                                "Hide"
                            } else {
                                "Show"
                            },
                            view,
                            &self.s1_open,
                        )),
                )
                .content(div().id("s1-section").test_support().child(picker)),
        )
    }

    pub(crate) fn s1_camouflage_section(&self, cx: &App) -> Option<AnyElement> {
        let network = Self::panel_value(&self.s1_network, cx, "tcp");
        let conditional = |id: &'static str, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let mut body = div().flex().flex_col().gap_1();
        let disguised = match network.as_str() {
            "ws" => {
                body = body
                    .child(conditional("s1-path", "Path", &self.s1_path))
                    .child(conditional("s1-host", "Host", &self.s1_host));
                true
            }
            "httpupgrade" | "http" => {
                body = body
                    .child(conditional("s1-host", "Host", &self.s1_host))
                    .child(conditional("s1-path", "Path", &self.s1_path));
                if network == "http" {
                    body = body.child(conditional("s1-method", "Method", &self.s1_method));
                }
                true
            }
            "grpc" => {
                body = body.child(conditional(
                    "s1-service-name",
                    "Service name",
                    &self.s1_service_name,
                ));
                true
            }
            _ => false,
        };
        if !disguised {
            return None;
        }
        Some(
            field()
                .label_indent(false)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .id("camouflage-caption")
                                .test_support()
                                .child(Label::new("Camouflage")),
                        )
                        .child(div().id("camouflage-section").test_support().child(body)),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn s2_tls_section(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        forced: bool,
    ) -> impl IntoElement {
        let _ = cx;
        let mut tls_body = div().flex().flex_col().gap_1();
        if forced {
            tls_body = tls_body.child(
                field().label_indent(false).child(
                    div()
                        .id("s2-forced-note")
                        .test_support()
                        .child("TLS is always on for this protocol (security locked tls)."),
                ),
            );
        } else {
            let s2_enable = self.s2_enable.clone();
            let s2_repaint = repaint.clone();
            tls_body = tls_body.child(
            field().label_indent(false).child(
                div().id("s2-enable").test_support().child(
                    Switch::new("s2-enable-switch")
                        .small()
                        .checked(*self.s2_enable.borrow())
                        .label("Enable TLS")
                        .tooltip(
                            "Wrap the stream in TLS (security=tls, or reality with a public key)",
                        )
                        .accessibility_label("Enable TLS for this row")
                        .on_change(move |next: &bool, window, cx| {
                            *s2_enable.borrow_mut() = *next;
                            s2_repaint(window, cx);
                        }),
                ),
            ),
        );
        }
        field().label_indent(false).child(
            Collapsible::new()
                .open(*self.s2_open.borrow())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id("s2-caption")
                                .test_support()
                                .child(Label::new(
                                    if forced { "TLS (always on)" } else { "TLS" }.to_string(),
                                )),
                        )
                        .child(panel_toggle_button(
                            "s2-toggle",
                            if *self.s2_open.borrow() {
                                "Hide"
                            } else {
                                "Show"
                            },
                            view,
                            &self.s2_open,
                        )),
                )
                .content(div().id("s2-section").test_support().child(tls_body)),
        )
    }

    pub(crate) fn s2_security_section(&self, cx: &App) -> impl IntoElement {
        let _ = cx;
        let fp_select = self.s2_fp.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                field().label("SNI".to_string()).child(
                    div()
                        .id("s2-sni")
                        .test_support()
                        .child(Input::new(&self.s2_sni)),
                ),
            )
            .child(
                field().label("ALPN (comma list)".to_string()).child(
                    div()
                        .id("s2-alpn")
                        .test_support()
                        .child(Input::new(&self.s2_alpn)),
                ),
            )
            .child(
                field().label("Fingerprint".to_string()).child(
                    div()
                        .id("s2-fp-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new("uTLS fingerprint".to_string()))
                                .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&fp_select)
                                .id("s2-fp")
                                .small()
                                .accessibility_label("uTLS fingerprint"),
                        ),
                ),
            );
        field().label_indent(false).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .id("tls-security-caption")
                        .test_support()
                        .child(Label::new("TLS Security")),
                )
                .child(div().id("tls-security-section").test_support().child(body)),
        )
    }

    pub(crate) fn s2_extras_section(&self, repaint: &DialogRepaint) -> impl IntoElement {
        let s2_insecure = self.s2_insecure.clone();
        let s2_repaint_insecure = repaint.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                field().label_indent(false).child(
                    div().id("s2-insecure").test_support().child(
                        Switch::new("s2-insecure-switch")
                            .small()
                            .checked(*self.s2_insecure.borrow())
                            .label("Allow insecure")
                            .tooltip("Skip certificate verification (testing only)")
                            .accessibility_label("Skip TLS certificate verification")
                            .on_change(move |next: &bool, window, cx| {
                                *s2_insecure.borrow_mut() = *next;
                                s2_repaint_insecure(window, cx);
                            }),
                    ),
                ),
            )
            .child(
                field().label("Reality public key".to_string()).child(
                    div()
                        .id("s2-pbk")
                        .test_support()
                        .child(Input::new(&self.s2_pbk)),
                ),
            )
            .child(
                field().label("Reality short id".to_string()).child(
                    div()
                        .id("s2-sid")
                        .test_support()
                        .child(Input::new(&self.s2_sid)),
                ),
            );
        field().label_indent(false).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .id("tls-extras-caption")
                        .test_support()
                        .child(Label::new("TLS extras")),
                )
                .child(div().id("tls-extras-section").test_support().child(body)),
        )
    }

    pub(crate) fn s3_section(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        vision: bool,
    ) -> AnyElement {
        let _ = cx;
        if vision {
            return field().label_indent(false).child(
                div()
                    .id("s3-vision-note")
                    .test_support()
                    .child("Mux is unavailable with the xtls-rprx-vision flow — stored mux keys are preserved."),
            ).into_any_element();
        }
        let effective = self.s3_effective_mode();
        let selected = ["", "on", "off"].iter().position(|mode| *mode == effective);
        let mode_labels = ["Keep Default", "On", "Off"];
        let mode_cell = self.s3_mode.clone();
        let mode_repaint = repaint.clone();
        let mut group = RadioGroup::horizontal("s3-mux-options")
            .selected_index(selected)
            .on_change(move |ix, window, cx| {
                if let Some(mode) = ["", "on", "off"].get(*ix) {
                    *mode_cell.borrow_mut() = mode.to_string();
                }
                mode_repaint(window, cx);
            });
        for label in mode_labels {
            group = group.child(
                Radio::new(label)
                    .small()
                    .label(label)
                    .tooltip(format!("Mux mode {label}")),
            );
        }
        let mut s3_body = div().flex().flex_col().gap_1().child(
            field().label("Multiplex mode".to_string()).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("s3-mux-caption")
                            .test_support()
                            .child(Label::new("Multiplex mode")),
                    )
                    .child(div().id("s3-mux-group").test_support().child(group)),
            ),
        );
        if *self.s3_brutal.borrow() && effective == "on" && self.s3_mode.borrow().as_str() != "on" {
            s3_body = s3_body.child(
                div()
                    .id("s3-brutal-note")
                    .test_support()
                    .child("Brutal requires mux (forced On)."),
            );
        }
        let brutal_cell = self.s3_brutal.clone();
        let brutal_repaint = repaint.clone();
        s3_body = s3_body
            .child(
                field().label_indent(false).child(
                    div().id("s3-brutal").test_support().child(
                        Switch::new("s3-brutal-switch")
                            .small()
                            .checked(*self.s3_brutal.borrow())
                            .label("Brutal (forces mux On)")
                            .tooltip("Brutal congestion speeds (Mbps); enabling forces mux On")
                            .accessibility_label("Enable Brutal speeds (forces mux On)")
                            .on_change(move |next: &bool, window, cx| {
                                *brutal_cell.borrow_mut() = *next;
                                brutal_repaint(window, cx);
                            }),
                    ),
                ),
            )
            .child(
                field().label("Brutal up Mbps".to_string()).child(
                    div()
                        .id("s3-up")
                        .test_support()
                        .child(Input::new(&self.s3_up)),
                ),
            )
            .child(
                field().label("Brutal down Mbps".to_string()).child(
                    div()
                        .id("s3-down")
                        .test_support()
                        .child(Input::new(&self.s3_down)),
                ),
            );
        field()
            .label_indent(false)
            .child(
                Collapsible::new()
                    .open(*self.s3_open.borrow())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .id("s3-caption")
                                    .test_support()
                                    .child(Label::new("Mux")),
                            )
                            .child(panel_toggle_button(
                                "s3-toggle",
                                if *self.s3_open.borrow() {
                                    "Hide"
                                } else {
                                    "Show"
                                },
                                view,
                                &self.s3_open,
                            )),
                    )
                    .content(div().id("s3-section").test_support().child(s3_body)),
            )
            .into_any_element()
    }

    pub(crate) fn s4_section(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let _ = cx;
        let dial_switch = |id: &'static str,
                           switch_id: &'static str,
                           label: &str,
                           tip: &str,
                           cell: &Rc<RefCell<bool>>| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let tip = tip.to_string();
            let label = label.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let mut s4_body = div().flex().flex_col().gap_1();
        s4_body = s4_body
            .child(dial_switch(
                "s4-reuse",
                "s4-reuse-switch",
                "Reuse address",
                "SO_REUSEADDR on the upstream socket",
                &self.s4_reuse,
            ))
            .child(dial_switch(
                "s4-fastopen",
                "s4-fastopen-switch",
                "TCP fast open",
                "TCP Fast Open on the upstream socket",
                &self.s4_fastopen,
            ))
            .child(dial_switch(
                "s4-fragment",
                "s4-fragment-switch",
                "UDP fragment",
                "Fragment upstream UDP packets",
                &self.s4_fragment,
            ))
            .child(dial_switch(
                "s4-multipath",
                "s4-multipath-switch",
                "TCP multipath",
                "Multipath TCP on the upstream socket",
                &self.s4_multipath,
            ))
            .child(
                field().label("Connect timeout".to_string()).child(
                    div()
                        .id("s4-timeout")
                        .test_support()
                        .child(Input::new(&self.s4_timeout)),
                ),
            )
            .child(
                field().label("Bind interface".to_string()).child(
                    div()
                        .id("s4-bind")
                        .test_support()
                        .child(Input::new(&self.s4_bind)),
                ),
            )
            .child(
                field().label("IPv4 bind address".to_string()).child(
                    div()
                        .id("s4-inet4")
                        .test_support()
                        .child(Input::new(&self.s4_inet4)),
                ),
            )
            .child(
                field().label("IPv6 bind address".to_string()).child(
                    div()
                        .id("s4-inet6")
                        .test_support()
                        .child(Input::new(&self.s4_inet6)),
                ),
            );

        field().label_indent(false).child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .id("s4-caption")
                        .test_support()
                        .child(Label::new("Dial Fields")),
                )
                .child(div().id("s4-section").test_support().child(s4_body)),
        )
    }

    pub(crate) fn vmess_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.vmess_uuid, "vmess", cx));
        block = block.child(
            field().label("Alter ID".to_string()).child(
                div()
                    .id("vmess-aid")
                    .test_support()
                    .child(Input::new(&self.vmess_aid)),
            ),
        );
        block = block.child(Self::panel_select_row(
            "Security",
            "vmess-security-tip",
            "VMess packet cipher (legacy links keep it in security)",
            "vmess-security",
            "VMess security (packet cipher)",
            &self.vmess_security,
        ));
        block = block.child(Self::panel_select_row(
            "Packet encoding",
            "vmess-packet-encoding-tip",
            "VMess packet encoding",
            "vmess-packet-encoding",
            "VMess packet encoding",
            &self.vmess_packet_encoding,
        ));
        block = block.child(self.s1_network_section(view, cx));
        block = block.child(self.s2_tls_section(view, repaint, cx, false));
        block = block.child(self.s3_section(view, repaint, cx, false));

        Self::page_disclosure(
            "VMess settings",
            "vmess-section-caption",
            "vmess-section-toggle",
            "vmess-section",
            &self.vmess_open,
            view,
            block,
        )
    }

    pub(crate) fn trojan_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(password_row(
            "Password *",
            "trojan-password",
            &self.trojan_password,
            cx,
        ));
        block = block.child(self.s1_network_section(view, cx));
        block = block.child(self.s2_tls_section(view, repaint, cx, false));
        block = block.child(self.s3_section(view, repaint, cx, false));

        Self::page_disclosure(
            "Trojan settings",
            "trojan-section-caption",
            "trojan-section-toggle",
            "trojan-section",
            &self.trojan_open,
            view,
            block,
        )
    }

    pub(crate) fn shadowsocks_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            "Method",
            "ss-method-tip",
            "Shadowsocks cipher (all 18)",
            "ss-method",
            "Shadowsocks method",
            &self.ss_method,
        ));
        block = block.child(password_row(
            "Password *",
            "ss-password",
            &self.ss_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            "Plugin",
            "ss-plugin-tip",
            "Plugin executable (none runs stock)",
            "ss-plugin",
            "Shadowsocks plugin",
            &self.ss_plugin,
        ));
        block = block.child(
            field().label("Plugin options".to_string()).child(
                div()
                    .id("ss-plugin-opts")
                    .test_support()
                    .child(Input::new(&self.ss_plugin_opts)),
            ),
        );
        let uot_cell = self.ss_uot.clone();
        let uot_repaint = repaint.clone();
        block = block.child(
            field().label_indent(false).child(
                div().id("ss-uot").test_support().child(
                    Switch::new("ss-uot-switch")
                        .small()
                        .checked(*self.ss_uot.borrow())
                        .label("UDP over TCP (uot)")
                        .tooltip("UDP over TCP for shadowsocks (folds the uot flat)")
                        .accessibility_label("UDP over TCP for shadowsocks")
                        .on_change(move |next: &bool, window, cx| {
                            *uot_cell.borrow_mut() = *next;
                            uot_repaint(window, cx);
                        }),
                ),
            ),
        );
        if !Self::panel_value(&self.ss_plugin, cx, "").trim().is_empty() {
            block = block.child(field().label_indent(false).child(
                div().id("ss-plugin-note").test_support().child(
                    "Plugin rows need the plugin executable beside the engine — stock check fails without it.",
                ),
            ));
        }
        block = block.child(self.s3_section(view, repaint, cx, false));

        Self::page_disclosure(
            "Shadowsocks settings",
            "ss-section-caption",
            "ss-section-toggle",
            "ss-section",
            &self.ss_open,
            view,
            block,
        )
    }

    pub(crate) fn wg_peer_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        peer: WgPeerRefs<'_>,
    ) -> impl IntoElement {
        let prefix = peer.prefix;
        let secret_entity = peer.private_key.clone();
        let keygen_view = view.clone();
        let key_presence =
            secret_state_caption(peer.private_key.read(cx).value().as_ref()).to_string();
        let sys_cell = peer.sys_ifc.clone();
        let sys_repaint = repaint.clone();
        let row = |id: String, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let masked_row = |id: String, label: &str, state: &Entity<InputState>| {
            field().label(label.to_string()).child(
                div()
                    .id(id)
                    .test_support()
                    .child(Input::new(state).mask_toggle()),
            )
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                field().label("Private key *".to_string()).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .id(format!("{prefix}-private-key"))
                                        .test_support()
                                        .child(Input::new(peer.private_key).mask_toggle()),
                                )
                                .child(
                                    Button::new(format!("{prefix}-private-key-generate"))
                                .small()
                                .label("Generate")
                                .tooltip(
                                    "Generate a fresh WireGuard private key (local only — no WARP registration)",
                                )
                                .accessibility_label("Generate a fresh WireGuard private key")
                                .on_click(move |_, window, cx| {
                                    let generated = generate_wireguard_key();
                                    keygen_view.update(cx, |_, cx| {
                                        secret_entity.update(cx, |state, cx| {
                                            state.set_value(generated, &mut *window, cx);
                                        });
                                    });
                                    window.push_notification(
                                        Notification::info("Generated a new WireGuard private key"),
                                        cx,
                                    );
                                }),
                        )
                        .child(
                            div()
                                .id(format!("{prefix}-private-key-state"))
                                .test_support()
                                .text_sm()
                                .child(key_presence),
                        ),
                ),
            ))
            .child(row(
                format!("{prefix}-public-key"),
                "Public key (peer)",
                peer.public_key,
            ))
            .child(masked_row(
                format!("{prefix}-preshared-key"),
                "Preshared key",
                peer.pre_shared_key,
            ))
            .child(row(
                format!("{prefix}-reserved"),
                "Reserved (comma ints)",
                peer.reserved,
            ))
            .child(row(
                format!("{prefix}-keepalive"),
                "Keepalive (seconds)",
                peer.keepalive,
            ))
            .child(row(
                format!("{prefix}-local-addr"),
                "Local address (comma CIDRs)",
                peer.local_addr,
            ))
            .child(row(format!("{prefix}-dns"), "DNS", peer.dns))
            .child(row(format!("{prefix}-mtu"), "MTU", peer.mtu))
            .child(row(
                format!("{prefix}-workers"),
                "Workers",
                peer.workers,
            ))
            .child(
                field().label_indent(false).child(
                    div()
                        .id(format!("{prefix}-sys-ifc"))
                        .test_support()
                        .child(
                            Switch::new(format!("{prefix}-sys-ifc-switch"))
                                .small()
                                .checked(*peer.sys_ifc.borrow())
                                .label("System interface (sys_ifc)")
                                .tooltip("Use the system WireGuard interface (stores as system)")
                                .accessibility_label("Use the system WireGuard interface")
                                .on_change(move |next: &bool, window, cx| {
                                    *sys_cell.borrow_mut() = *next;
                                    sys_repaint(window, cx);
                                }),
                        ),
                ),
            )
    }

    pub(crate) fn wireguard_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.wg_peer_block(
            view,
            repaint,
            cx,
            WgPeerRefs {
                prefix: "wg",
                private_key: &self.wg_private,
                public_key: &self.wg_public,
                pre_shared_key: &self.wg_preshared,
                reserved: &self.wg_reserved,
                keepalive: &self.wg_keepalive,
                local_addr: &self.wg_local_addr,
                dns: &self.wg_dns,
                mtu: &self.wg_mtu,
                workers: &self.wg_workers,
                sys_ifc: &self.wg_sys_ifc,
            },
        ));

        block = block.child(field().label_indent(false).child(
            div().id("wg-notes").test_support().child(
                "WireGuard rows become endpoints[] at generate. Blank AllowedIPs defaults to \
                 0.0.0.0/0; DNS is stored but never emitted; udp_timeout stays JSON-only.",
            ),
        ));
        Self::page_disclosure(
            "WireGuard settings",
            "wg-section-caption",
            "wg-section-toggle",
            "wg-section",
            &self.wg_open,
            view,
            block,
        )
    }

    pub(crate) fn amneziawg_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let row = |id: &'static str, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let caption = |id: &'static str, text: &str| {
            div()
                .id(id)
                .test_support()
                .child(Label::new(text.to_string()))
        };
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.wg_peer_block(
            view,
            repaint,
            cx,
            WgPeerRefs {
                prefix: "awg",
                private_key: &self.awg_private,
                public_key: &self.awg_public,
                pre_shared_key: &self.awg_preshared,
                reserved: &self.awg_reserved,
                keepalive: &self.awg_keepalive,
                local_addr: &self.awg_local_addr,
                dns: &self.awg_dns,
                mtu: &self.awg_mtu,
                workers: &self.awg_workers,
                sys_ifc: &self.awg_sys_ifc,
            },
        ));
        block = block.child(switch_row(
            "awg-enable",
            "awg-enable-switch".to_string(),
            "Enable Amnezia",
            "AmneziaWG obfuscation marker (UI state — stripped at generate, like brutal speeds)",
            &self.awg_enable,
            repaint,
        ));
        block = block.child(Self::panel_select_row(
            "AWG version",
            "awg-version-tip",
            "AmneziaWG version (2.0 or 3.x — gates the v3 fields below)",
            "awg-version",
            "AmneziaWG version",
            &self.awg_version,
        ));
        block = block.child(caption("awg-junk-caption", "Junk train"));
        block = block
            .child(row("awg-jc", "Jc", &self.awg_jc))
            .child(row("awg-jmin", "Jmin", &self.awg_jmin))
            .child(row("awg-jmax", "Jmax", &self.awg_jmax));
        block = block.child(caption("awg-length-caption", "Length prefixes (S1–S4)"));
        block = block
            .child(row("awg-s1", "S1", &self.awg_s1))
            .child(row("awg-s2", "S2", &self.awg_s2))
            .child(row("awg-s3", "S3", &self.awg_s3))
            .child(row("awg-s4", "S4", &self.awg_s4));
        block = block.child(caption("awg-header-caption", "Header ranges (H1–H4)"));
        block = block
            .child(row("awg-h1", "H1", &self.awg_h1))
            .child(row("awg-h2", "H2", &self.awg_h2))
            .child(row("awg-h3", "H3", &self.awg_h3))
            .child(row("awg-h4", "H4", &self.awg_h4));
        block = block.child(caption("awg-chain-caption", "CPS chains (I1–I5)"));
        block = block
            .child(row("awg-i1", "I1", &self.awg_i1))
            .child(row("awg-i2", "I2", &self.awg_i2))
            .child(row("awg-i3", "I3", &self.awg_i3))
            .child(row("awg-i4", "I4", &self.awg_i4))
            .child(row("awg-i5", "I5", &self.awg_i5));

        if Self::panel_value(&self.awg_version, cx, "") == "3.x" {
            block = block.child(caption("awg-v3-caption", "v3-only fields"));
            block = block
                .child(row("awg-hpk", "Header protection key", &self.awg_hpk))
                .child(row("awg-pad", "Content padding addition", &self.awg_pad))
                .child(row(
                    "awg-rekey-after",
                    "Rekey after time",
                    &self.awg_rekey_after,
                ))
                .child(row(
                    "awg-rekey-timeout",
                    "Rekey timeout",
                    &self.awg_rekey_timeout,
                ))
                .child(row(
                    "awg-reject-after",
                    "Reject after time",
                    &self.awg_reject_after,
                ))
                .child(row(
                    "awg-keepalive-timeout",
                    "Keepalive timeout",
                    &self.awg_keepalive_timeout,
                ))
                .child(row(
                    "awg-max-handshake",
                    "Max handshake attempts",
                    &self.awg_max_handshake,
                ));
            block = block.child(switch_row(
                "awg-trailers",
                "awg-trailers-switch".to_string(),
                "Random trailers",
                "Append random trailers (v3-only — passes through for the fork check)",
                &self.awg_trailers,
                repaint,
            ));
            block = block.child(switch_row(
                "awg-cookies",
                "awg-cookies-switch".to_string(),
                "Disable cookies",
                "Disable handshake cookies (v3-only — passes through for the fork check)",
                &self.awg_cookies,
                repaint,
            ));
        } else {
            block = block.child(
                field().label_indent(false).child(
                    div()
                        .id("awg-v3-note")
                        .test_support()
                        .child("v3-only fields hidden — stored values preserved (never wiped)."),
                ),
            );
        }

        block = block.child(field().label_indent(false).child(
            div().id("awg-notes").test_support().child(
                "AmneziaWG rows run in the fork sidecar (sing-box-awg). v3-only keys on a 2.0 row \
                 fail generate naming the field; udp_timeout stays JSON-only.",
            ),
        ));
        Self::page_disclosure(
            "AmneziaWG settings",
            "awg-section-caption",
            "awg-section-toggle",
            "awg-section",
            &self.awg_open,
            view,
            block,
        )
    }

    pub(crate) fn hysteria_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let row = |id: &'static str, label: &str, state: &Entity<InputState>| {
            field()
                .label(label.to_string())
                .child(div().id(id).test_support().child(Input::new(state)))
        };
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let protocol = self.protocol_text(cx);
        let is_v2 = protocol == "hysteria2";
        let mut block = div().flex().flex_col().gap_2();
        if self.protocol_locked {
            block = block.child(
                field().label("Protocol version".to_string()).child(
                    div()
                        .id("hy-version-locked")
                        .test_support()
                        .child(if is_v2 { "2 (locked)" } else { "1 (locked)" }),
                ),
            );
        } else {
            let picker = self.protocol_select.clone();
            let version_repaint = repaint.clone();
            let selected = Some(if is_v2 { 1 } else { 0 });
            let mut group = RadioGroup::horizontal("hy-version-options")
                .selected_index(selected)
                .on_change(move |ix, window, cx| {
                    let want = if *ix == 1 { "hysteria2" } else { "hysteria" };
                    let next = want.to_string();
                    picker.update(cx, |state, cx| {
                        state.set_selected_value(&next, window, cx);
                    });
                    version_repaint(window, cx);
                });
            for label in ["1", "2"] {
                group = group.child(
                    Radio::new(label)
                        .small()
                        .label(label)
                        .tooltip(format!("Hysteria version {label}")),
                );
            }
            block = block.child(
                field().label("Protocol version".to_string()).child(
                    div()
                        .id("hy-version-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                Tooltip::new(
                                    "Hysteria protocol version (switches layout)".to_string(),
                                )
                            })
                            .into()
                        })
                        .child(div().id("hy-version-group").test_support().child(group)),
                ),
            );
        }
        block = block
            .child(row(
                "hy-server-ports",
                "Server ports (e.g. 9000-9010)",
                &self.hy_server_ports,
            ))
            .child(row(
                "hy-hop-interval",
                "Hop interval (e.g. 30s)",
                &self.hy_hop_interval,
            ))
            .child(row("hy-up", "Up Mbps", &self.hy_up))
            .child(row("hy-down", "Down Mbps", &self.hy_down))
            .child(row("hy-obfs", "Obfs", &self.hy_obfs));
        if is_v2 {
            block = block.child(password_row(
                "Password *",
                "hy-password",
                &self.hy_password,
                cx,
            ));
        } else {
            block = block.child(Self::panel_select_row(
                "Auth type",
                "hy-auth-type-tip",
                "v1 auth marker (blank leaves it unset)",
                "hy-auth-type",
                "Hysteria auth type",
                &self.hy_auth_type,
            ));
            block = block.child(password_row("Auth *", "hy-auth", &self.hy_auth, cx));
            block = block
                .child(row(
                    "hy-recv-window-conn",
                    "Recv window conn",
                    &self.hy_recv_conn,
                ))
                .child(row("hy-recv-window", "Recv window", &self.hy_recv));
            block = block.child(switch_row(
                "hy-mtu",
                "hy-mtu-switch".to_string(),
                "Disable MTU discovery",
                "Disable MTU discovery on v1 (folds disable_mtu_discovery)",
                &self.hy_mtu,
                repaint,
            ));
        }
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("hy-notes").test_support().child(
                "Hysteria v1 rows require up/down (blank refuses naming up_mbps/down_mbps); \
                 v2 omits them for BBR. v1-only keys on a v2 row fail generate naming the field.",
            ),
        ));
        Self::page_disclosure(
            "Hysteria settings",
            "hy-section-caption",
            "hy-section-toggle",
            "hy-section",
            &self.hy_open,
            view,
            block,
        )
    }

    pub(crate) fn tuic_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.tuic_uuid, "tuic", cx));
        block = block.child(password_row(
            "Password *",
            "tuic-password",
            &self.tuic_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            "Congestion control",
            "tuic-cc-tip",
            "TUIC congestion control (blank leaves it unset)",
            "tuic-cc",
            "TUIC congestion control",
            &self.tuic_cc,
        ));
        block = block.child(Self::panel_select_row(
            "UDP relay mode",
            "tuic-relay-tip",
            "TUIC relay mode (blank leaves it unset)",
            "tuic-relay",
            "TUIC UDP relay mode",
            &self.tuic_relay,
        ));
        block = block.child(switch_row(
            "tuic-uot",
            "tuic-uot-switch".to_string(),
            "UDP over stream",
            "UDP over stream (folds udp_over_stream)",
            &self.tuic_uot,
            repaint,
        ));
        block = block.child(switch_row(
            "tuic-zero-rtt",
            "tuic-zero-rtt-switch".to_string(),
            "Zero RTT",
            "Zero-RTT handshake (folds zero_rtt_handshake)",
            &self.tuic_zero_rtt,
            repaint,
        ));
        block = block.child(
            field().label("Heartbeat".to_string()).child(
                div()
                    .id("tuic-heartbeat")
                    .test_support()
                    .child(Input::new(&self.tuic_heartbeat)),
            ),
        );
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("tuic-notes").test_support().child(
                "TUIC runs TLS always-on (security locked tls). Blank congestion/relay stay unset.",
            ),
        ));
        Self::page_disclosure(
            "TUIC settings",
            "tuic-section-caption",
            "tuic-section-toggle",
            "tuic-section",
            &self.tuic_open,
            view,
            block,
        )
    }

    pub(crate) fn socks_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let uot_cell = self.socks_uot.clone();
        let uot_repaint = repaint.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            "Version",
            "socks-version-tip",
            "SOCKS version (blank leaves the stock default 5)",
            "socks-version",
            "SOCKS version",
            &self.socks_version,
        ));
        block = block.child(
            field().label("Username".to_string()).child(
                div()
                    .id("socks-username")
                    .test_support()
                    .child(Input::new(&self.socks_username)),
            ),
        );
        block = block.child(password_row(
            "Password",
            "socks-password",
            &self.socks_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            "Network",
            "socks-network-tip",
            "SOCKS network (tcp is the default)",
            "socks-network",
            "SOCKS network",
            &self.socks_network,
        ));
        block = block.child(
            field().label_indent(false).child(
                div().id("socks-uot").test_support().child(
                    Switch::new("socks-uot-switch")
                        .small()
                        .checked(*self.socks_uot.borrow())
                        .label("UDP over TCP (uot)")
                        .tooltip("UDP over TCP for socks (folds the uot flat)")
                        .accessibility_label("UDP over TCP for socks")
                        .on_change(move |next: &bool, window, cx| {
                            *uot_cell.borrow_mut() = *next;
                            uot_repaint(window, cx);
                        }),
                ),
            ),
        );

        block = block.child(field().label_indent(false).child(
            div().id("socks-notes").test_support().child(
                "SOCKS serves version 5 stock (blank leaves the default); 4 fails generate \
                 naming version. Blank network leaves tcp.",
            ),
        ));
        Self::page_disclosure(
            "SOCKS settings",
            "socks-section-caption",
            "socks-section-toggle",
            "socks-section",
            &self.socks_open,
            view,
            block,
        )
    }

    pub(crate) fn http_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label("Username".to_string()).child(
                    div()
                        .id("http-username")
                        .test_support()
                        .child(Input::new(&self.http_username)),
                ),
            )
            .child(password_row(
                "Password",
                "http-password",
                &self.http_password,
                cx,
            ))
            .child(
                field().label("Path".to_string()).child(
                    div()
                        .id("http-path")
                        .test_support()
                        .child(Input::new(&self.http_path)),
                ),
            )
            .child(
                field().label("Headers".to_string()).child(
                    div()
                        .id("http-headers")
                        .test_support()
                        .child(Input::new(&self.http_headers)),
                ),
            );
        block = block.child(self.s2_tls_section(view, repaint, cx, false));

        block = block.child(field().label_indent(false).child(
            div().id("http-notes").test_support().child(
                "HTTPS rows serve TLS-on (security=tls) even with the panel off. \
                 Path/headers ride the http outbound — check judges the headers shape.",
            ),
        ));
        Self::page_disclosure(
            "HTTP settings",
            "http-section-caption",
            "http-section-toggle",
            "http-section",
            &self.http_open,
            view,
            block,
        )
    }

    pub(crate) fn anytls_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(password_row(
            "Password *",
            "anytls-password",
            &self.anytls_password,
            cx,
        ));
        block = block
            .child(
                field().label("Idle check interval".to_string()).child(
                    div()
                        .id("anytls-interval")
                        .test_support()
                        .child(Input::new(&self.anytls_interval)),
                ),
            )
            .child(
                field().label("Idle timeout".to_string()).child(
                    div()
                        .id("anytls-timeout")
                        .test_support()
                        .child(Input::new(&self.anytls_timeout)),
                ),
            )
            .child(
                field().label("Min idle sessions".to_string()).child(
                    div()
                        .id("anytls-min")
                        .test_support()
                        .child(Input::new(&self.anytls_min)),
                ),
            );
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("anytls-notes").test_support().child(
                "AnyTLS runs TLS always-on (security locked tls). Interval/timeout are \
                 durations (e.g. 30s); min is a session count.",
            ),
        ));
        Self::page_disclosure(
            "AnyTLS settings",
            "anytls-section-caption",
            "anytls-section-toggle",
            "anytls-section",
            &self.anytls_open,
            view,
            block,
        )
    }

    pub(crate) fn naive_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: &str,
                          tip: &str,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
            let label = label.to_string();
            let tip = tip.to_string();
            field().label_indent(false).child(
                div().id(id).test_support().child(
                    Switch::new(switch_id)
                        .small()
                        .checked(*cell.borrow())
                        .label(label.clone())
                        .tooltip(tip.clone())
                        .accessibility_label(label)
                        .on_change(move |next: &bool, window, cx| {
                            *switch_cell.borrow_mut() = *next;
                            switch_repaint(window, cx);
                        }),
                ),
            )
        };
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label("Username *".to_string()).child(
                    div()
                        .id("naive-username")
                        .test_support()
                        .child(Input::new(&self.naive_username)),
                ),
            )
            .child(password_row(
                "Password *",
                "naive-password",
                &self.naive_password,
                cx,
            ));
        block = block.child(switch_row(
            "naive-uot",
            "naive-uot-switch".to_string(),
            "UDP over TCP (uot)",
            "UDP over TCP for naive (folds the uot flat)",
            &self.naive_uot,
            repaint,
        ));
        block = block.child(switch_row(
            "naive-quic",
            "naive-quic-switch".to_string(),
            "QUIC",
            "Run naive over QUIC instead of HTTP/2",
            &self.naive_quic,
            repaint,
        ));
        block = block.child(Self::panel_select_row(
            "Congestion control",
            "naive-cc-tip",
            "QUIC congestion control (blank leaves the chromium default cubic)",
            "naive-cc",
            "Naive congestion control",
            &self.naive_cc,
        ));
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("naive-notes").test_support().child(
                "Naïve runs TLS always-on (security locked tls). Blank congestion control \
                 leaves the default (cubic).",
            ),
        ));
        Self::page_disclosure(
            "Naïve settings",
            "naive-section-caption",
            "naive-section-toggle",
            "naive-section",
            &self.naive_open,
            view,
            block,
        )
    }

    pub(crate) fn ssh_block(
        &self,
        view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let browse_view = view.clone();
        let browse_path = self.ssh_key_path.clone();
        let browse_key = self.ssh_private_key.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label("User".to_string()).child(
                    div()
                        .id("ssh-user")
                        .test_support()
                        .child(Input::new(&self.ssh_user)),
                ),
            )
            .child(password_row(
                "Password",
                "ssh-password",
                &self.ssh_password,
                cx,
            ))
            .child(
                field().label("Private key".to_string()).child(
                    div()
                        .id("ssh-private-key")
                        .test_support()
                        .child(Textarea::new(&self.ssh_private_key).h(px(96.))),
                ),
            )
            .child(
                field().label("Private key path".to_string()).child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id("ssh-key-path")
                                .test_support()
                                .child(Input::new(&self.ssh_key_path)),
                        )
                        .child(
                            Button::new("ssh-key-browse")
                                .small()
                                .label("Browse…")
                                .tooltip(
                                    "Pick a private-key file — fills the path and loads the key",
                                )
                                .accessibility_label("Browse for an SSH private-key file")
                                .on_click(move |_, window, cx| {
                                    browse_view.update(cx, |view, cx| {
                                        view.browse_for_ssh_key(
                                            browse_path.clone(),
                                            browse_key.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        ),
                ),
            )
            .child(
                field().label("Private key passphrase".to_string()).child(
                    div()
                        .id("ssh-key-pass")
                        .test_support()
                        .child(Input::new(&self.ssh_key_pass).mask_toggle()),
                ),
            )
            .child(
                field().label("Host key (comma list)".to_string()).child(
                    div()
                        .id("ssh-host-key")
                        .test_support()
                        .child(Input::new(&self.ssh_host_key)),
                ),
            )
            .child(
                field()
                    .label("Host key algorithms (comma list)".to_string())
                    .child(
                        div()
                            .id("ssh-host-key-algs")
                            .test_support()
                            .child(Input::new(&self.ssh_host_key_algs)),
                    ),
            )
            .child(
                field().label("Client version".to_string()).child(
                    div()
                        .id("ssh-client-version")
                        .test_support()
                        .child(Input::new(&self.ssh_client_version)),
                ),
            );

        block = block.child(field().label_indent(false).child(
            div().id("ssh-notes").test_support().child(
                "Browse fills the path and loads the key file (no hand-paste). Host key \
                 lists are comma-separated; cipher/mac/kex stay JSON-only.",
            ),
        ));
        Self::page_disclosure(
            "SSH settings",
            "ssh-section-caption",
            "ssh-section-toggle",
            "ssh-section",
            &self.ssh_open,
            view,
            block,
        )
    }

    pub(crate) fn snell_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let reuse_cell = self.snell_reuse.clone();
        let reuse_repaint = repaint.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(password_row("PSK *", "snell-psk", &self.snell_psk, cx))
            .child(Self::panel_select_row(
                "Version",
                "snell-version-tip",
                "Snell version (blank leaves the engine default 4)",
                "snell-version",
                "Snell version",
                &self.snell_version,
            ))
            .child(
                field().label("Obfs mode".to_string()).child(
                    div()
                        .id("snell-obfs-mode")
                        .test_support()
                        .child(Input::new(&self.snell_obfs_mode)),
                ),
            )
            .child(
                field().label("Obfs host".to_string()).child(
                    div()
                        .id("snell-obfs-host")
                        .test_support()
                        .child(Input::new(&self.snell_obfs_host)),
                ),
            )
            .child(
                field().label("User key".to_string()).child(
                    div()
                        .id("snell-userkey")
                        .test_support()
                        .child(Input::new(&self.snell_userkey)),
                ),
            )
            .child(
                field().label("Network".to_string()).child(
                    div()
                        .id("snell-network")
                        .test_support()
                        .child(Input::new(&self.snell_network)),
                ),
            )
            .child(
                field().label("Mode".to_string()).child(
                    div()
                        .id("snell-mode")
                        .test_support()
                        .child(Input::new(&self.snell_mode)),
                ),
            );
        block = block.child(
            field().label_indent(false).child(
                div().id("snell-reuse").test_support().child(
                    Switch::new("snell-reuse-switch")
                        .small()
                        .checked(*self.snell_reuse.borrow())
                        .label("Reuse")
                        .tooltip("Reuse sessions for snell (folds the reuse flat)")
                        .accessibility_label("Reuse sessions for snell")
                        .on_change(move |next: &bool, window, cx| {
                            *reuse_cell.borrow_mut() = *next;
                            reuse_repaint(window, cx);
                        }),
                ),
            ),
        );

        block = block.child(field().label_indent(false).child(
            div().id("snell-notes").test_support().child(
                "Snell serves version 4 when blank; 6 needs a v6 peer. \
                 Obfs/userkey/reuse/network/mode ride the snell outbound — check judges their shapes.",
            ),
        ));
        Self::page_disclosure(
            "Snell settings",
            "snell-section-caption",
            "snell-section-toggle",
            "snell-section",
            &self.snell_open,
            view,
            block,
        )
    }

    pub(crate) fn shadowtls_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(Self::panel_select_row(
                "Version",
                "shadowtls-version-tip",
                "ShadowTLS version (blank takes the schema default 1; v1 rows omit the password)",
                "shadowtls-version",
                "ShadowTLS version",
                &self.shadowtls_version,
            ))
            .child(password_row(
                "Password",
                "shadowtls-password",
                &self.shadowtls_password,
                cx,
            ));
        block = block.child(self.s2_tls_section(view, repaint, cx, true));

        block = block.child(field().label_indent(false).child(
            div().id("shadowtls-notes").test_support().child(
                "ShadowTLS runs TLS always-on (security locked tls). Version blank takes \
                 the schema default 1; v1 rows omit the password.",
            ),
        ));
        Self::page_disclosure(
            "ShadowTLS settings",
            "shadowtls-section-caption",
            "shadowtls-section-toggle",
            "shadowtls-section",
            &self.shadowtls_open,
            view,
            block,
        )
    }

    pub(crate) fn xray_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        _cx: &App,
    ) -> impl IntoElement {
        let _ = repaint;
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.xray_uuid, "xray", _cx));
        block = block
            .child(
                field().label("Encryption".to_string()).child(
                    div()
                        .id("xray-enc")
                        .test_support()
                        .child(Input::new(&self.xray_enc)),
                ),
            )
            .child(Self::panel_select_row(
                "Flow",
                "xray-flow-tip",
                "Xray flow (blank leaves the default)",
                "xray-flow",
                "Xray flow",
                &self.xray_flow,
            ))
            .child(
                field().label("SNI".to_string()).child(
                    div()
                        .id("xray-sni")
                        .test_support()
                        .child(Input::new(&self.xray_sni)),
                ),
            )
            .child(Self::panel_select_row(
                "Fingerprint",
                "xray-fp-tip",
                "Xray fingerprint (blank leaves the default)",
                "xray-fp",
                "Xray fingerprint",
                &self.xray_fp,
            ))
            .child(
                field().label("ALPN (comma list)".to_string()).child(
                    div()
                        .id("xray-alpn")
                        .test_support()
                        .child(Input::new(&self.xray_alpn)),
                ),
            )
            .child(
                field().label("REALITY public key (pbk)".to_string()).child(
                    div()
                        .id("xray-pbk")
                        .test_support()
                        .child(Input::new(&self.xray_pbk)),
                ),
            )
            .child(
                field().label("REALITY short id (sid)".to_string()).child(
                    div()
                        .id("xray-sid")
                        .test_support()
                        .child(Input::new(&self.xray_sid)),
                ),
            )
            .child(
                field().label("REALITY spiderX path".to_string()).child(
                    div()
                        .id("xray-spiderx")
                        .test_support()
                        .child(Input::new(&self.xray_spiderx)),
                ),
            )
            .child(Self::panel_select_row(
                "Network",
                "xray-network-tip",
                "Xray stream network (raw|xhttp|ws|httpupgrade|grpc)",
                "xray-network",
                "Xray network",
                &self.xray_network,
            ))
            .child(
                field().label("Mode".to_string()).child(
                    div()
                        .id("xray-mode")
                        .test_support()
                        .child(Input::new(&self.xray_mode)),
                ),
            )
            .child(Self::panel_select_row(
                "Mux",
                "xray-mux-tip",
                "Xray mux (Keep default leaves the sidecar default)",
                "xray-mux",
                "Xray mux",
                &self.xray_mux,
            ));
        block = block.child(field().label_indent(false).child(
            div().id("xray-notes").test_support().child(
                "Ph1 subset only — XHTTP/xmux/session keys (xPaddingBytes, maxConcurrency, \
                 downloadSettings…) stay JSON-only and are preserved, never stripped.",
            ),
        ));
        Self::page_disclosure(
            "Xray settings",
            "xray-section-caption",
            "xray-section-toggle",
            "xray-section",
            &self.xray_open,
            view,
            block,
        )
    }

    pub(crate) fn chain_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        _cx: &App,
    ) -> impl IntoElement {
        let hops = self.chain_hops.borrow().clone();
        let library = self.chain_library.borrow().clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            "Hop from library",
            "chain-pick-tip",
            "Add or replace hops from your row list (chain and off-index-0 extracore hops are rejected)",
            "chain-pick",
            "Chain hop from library",
            &self.chain_pick,
        ));
        {
            let add_view = view.clone();
            let add_hops = self.chain_hops.clone();
            let add_library = self.chain_library.clone();
            let add_pick = self.chain_pick.clone();
            let add_repaint = repaint.clone();
            block = block.child(
                field().label_indent(false).child(
                    div().id("chain-hop-add-row").test_support().child(
                        Button::new("chain-hop-add")
                            .small()
                            .label("Add hop")
                            .tooltip("Append the picked row to the chain")
                            .accessibility_label("Add the picked row to the chain")
                            .on_click(move |_, window, cx| {
                                let picked = add_pick.read(cx).selected_value().cloned();
                                let picked = picked.as_deref().unwrap_or("").to_string();
                                add_view.update(cx, |view, cx| {
                                    view.chain_add_hop(
                                        &add_hops,
                                        &add_library,
                                        &add_pick,
                                        &picked,
                                        window,
                                        cx,
                                    );
                                });
                                add_repaint(window, cx);
                            }),
                    ),
                ),
            );
        }
        if hops.is_empty() {
            block = block.child(
                div().id("chain-hops-empty").test_support().child(
                    Empty::new().header(
                        EmptyHeader::new()
                            .title(EmptyTitle::new().child("No hops yet"))
                            .description(EmptyDescription::new().child(
                                "Add hops from the library — the first hop may be an extra-core row.",
                            )),
                    ),
                ),
            );
        } else {
            let mut list = div()
                .flex()
                .flex_col()
                .gap_1()
                .id("chain-hops-list")
                .test_support();
            for (index, node) in hops.iter().enumerate() {
                let label = chain_hop_label(node, index, &library);
                let replace_view = view.clone();
                let replace_hops = self.chain_hops.clone();
                let replace_library = self.chain_library.clone();
                let replace_pick = self.chain_pick.clone();
                let replace_repaint = repaint.clone();
                let remove_hops = self.chain_hops.clone();
                let remove_repaint = repaint.clone();
                list = list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .id(format!("chain-hop-{index}"))
                                .test_support()
                                .child(Label::new(label)),
                        )
                        .child(
                            Button::new(format!("chain-hop-replace-{index}"))
                                .small()
                                .label("Replace")
                                .tooltip("Replace this hop with the picked row")
                                .accessibility_label(format!("Replace hop {}", index + 1))
                                .on_click(move |_, window, cx| {
                                    let picked = replace_pick.read(cx).selected_value().cloned();
                                    let picked = picked.as_deref().unwrap_or("").to_string();
                                    replace_view.update(cx, |view, cx| {
                                        view.chain_replace_hop(
                                            &replace_hops,
                                            &replace_library,
                                            &replace_pick,
                                            index,
                                            &picked,
                                            window,
                                            cx,
                                        );
                                    });
                                    replace_repaint(window, cx);
                                }),
                        )
                        .child(
                            Button::new(format!("chain-hop-remove-{index}"))
                                .small()
                                .label("Remove")
                                .tooltip("Remove this hop from the chain")
                                .accessibility_label(format!("Remove hop {}", index + 1))
                                .on_click(move |_, window, cx| {
                                    let mut hops = remove_hops.borrow_mut();
                                    if index < hops.len() {
                                        hops.remove(index);
                                    }
                                    drop(hops);
                                    remove_repaint(window, cx);
                                }),
                        ),
                );
            }
            block = block.child(list);
        }
        block = block.child(field().label_indent(false).child(
            div().id("chain-notes").test_support().child(
                "V1 chains are flat: no chain hops; at most one extra-core hop, first only. \
                 Extra/Custom/Xray hops must be last — the engine names violations at start.",
            ),
        ));
        Self::page_disclosure(
            "Chain settings",
            "chain-section-caption",
            "chain-section-toggle",
            "chain-section",
            &self.chain_open,
            view,
            block,
        )
    }

    pub(crate) fn extra_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let _ = (repaint, cx);
        let browse_view = view.clone();
        let browse_path = self.extra_exe.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field()
                    .label("Executable (app dir file name)".to_string())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .id("extra-exe")
                                    .test_support()
                                    .child(Input::new(&self.extra_exe)),
                            )
                            .child(
                                Button::new("extra-exe-browse")
                                    .small()
                                    .label("Browse…")
                                    .tooltip("Pick the sidecar executable — fills the path")
                                    .accessibility_label("Browse for the extra-core executable")
                                    .on_click(move |_, window, cx| {
                                        browse_view.update(cx, |view, cx| {
                                            view.browse_for_extra_exe(
                                                browse_path.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                            ),
                    ),
            )
            .child(
                field().label("Args".to_string()).child(
                    div()
                        .id("extra-args")
                        .test_support()
                        .child(Input::new(&self.extra_args)),
                ),
            )
            .child(
                field()
                    .label("Config (passed as -c <file>)".to_string())
                    .child(
                        div()
                            .id("extra-config")
                            .test_support()
                            .child(Textarea::new(&self.extra_config).h(px(96.))),
                    ),
            );
        block = block.child(field().label_indent(false).child(
            div().id("extra-notes").test_support().child(
                "Spawned unprivileged as <exe> -c <config-file> (v1 convention — extra flags \
                 ride args with the row). Validate checks exe + config shape before commit; \
                 there is no logs toggle by design.",
            ),
        ));
        Self::page_disclosure(
            "Extra-core settings",
            "extra-section-caption",
            "extra-section-toggle",
            "extra-section",
            &self.extra_open,
            view,
            block,
        )
    }

    pub(crate) fn vless_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();

        block = block.child(self.uuid_row(view, &self.vless_uuid, "vless", cx));
        let flow_select = self.vless_flow.clone();
        let packet_select = self.vless_packet_encoding.clone();
        block = block.child(
            field().label("Flow".to_string()).child(
                div()
                    .id("vless-flow-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Tooltip::new("VLESS flow (vision disables mux)".to_string()))
                            .into()
                    })
                    .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                    })
                    .child(
                        Select::new(&flow_select)
                            .id("vless-flow")
                            .small()
                            .accessibility_label("VLESS flow"),
                    ),
            ),
        );
        block = block.child(
            field().label("Packet encoding".to_string()).child(
                div()
                    .id("vless-packet-encoding-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Tooltip::new("VLESS packet encoding".to_string()))
                            .into()
                    })
                    .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                    })
                    .child(
                        Select::new(&packet_select)
                            .id("vless-packet-encoding")
                            .small()
                            .accessibility_label("VLESS packet encoding"),
                    ),
            ),
        );

        block = block.child(self.s1_network_section(view, cx));

        block = block.child(self.s2_tls_section(view, repaint, cx, false));

        let flow = Self::panel_value(&self.vless_flow, cx, "");
        block = block.child(self.s3_section(view, repaint, cx, flow == "xtls-rprx-vision"));

        Self::page_disclosure(
            "VLESS settings",
            "vless-section-caption",
            "vless-section-toggle",
            "vless-section",
            &self.vless_open,
            view,
            block,
        )
    }

    pub(crate) fn element(
        self,
        view: Entity<AppView>,
        cx: &App,
        collision: Option<String>,
        form_error: Option<(String, String)>,
        show_test_url: bool,
        port_budget: Option<String>,
    ) -> impl IntoElement {
        let bind = self.bind.read(cx).value().to_string();
        let protocol_text = self.protocol_text(cx);
        let guided = guided_identity_fields(&protocol_text);

        let virtual_row = is_virtual_protocol(&protocol_text);
        let format_view = view.clone();
        let format_params = self.params.clone();
        let template_view = view.clone();
        let template_params = self.params.clone();
        let template_form = self.clone();
        let validate_view = view.clone();
        let validate_params = self.params.clone();
        let validate_form = self.clone();
        let expanded = *self.advanced.borrow();

        let toggle_form = self.clone();

        let guided_panels = if expanded {
            None
        } else {
            let repaint_view = view.clone();
            let repaint: DialogRepaint = Rc::new(move |_window, cx| {
                repaint_view.update(cx, |_, cx| cx.notify());
            });
            match protocol_text.as_str() {
                "vless" => Some(self.vless_block(&view, &repaint, cx).into_any_element()),
                "vmess" => Some(self.vmess_block(&view, &repaint, cx).into_any_element()),
                "trojan" => Some(self.trojan_block(&view, &repaint, cx).into_any_element()),
                "shadowsocks" => Some(
                    self.shadowsocks_block(&view, &repaint, cx)
                        .into_any_element(),
                ),
                "wireguard" => Some(self.wireguard_block(&view, &repaint, cx).into_any_element()),
                "amneziawg" => Some(self.amneziawg_block(&view, &repaint, cx).into_any_element()),
                "hysteria" | "hysteria2" => {
                    Some(self.hysteria_block(&view, &repaint, cx).into_any_element())
                }
                "tuic" => Some(self.tuic_block(&view, &repaint, cx).into_any_element()),

                "socks" => Some(self.socks_block(&view, &repaint, cx).into_any_element()),
                "http" | "https" => Some(self.http_block(&view, &repaint, cx).into_any_element()),
                "anytls" => Some(self.anytls_block(&view, &repaint, cx).into_any_element()),
                "naive" => Some(self.naive_block(&view, &repaint, cx).into_any_element()),
                "ssh" => Some(self.ssh_block(&view, &repaint, cx).into_any_element()),

                "snell" => Some(self.snell_block(&view, &repaint, cx).into_any_element()),
                "shadowtls" => Some(self.shadowtls_block(&view, &repaint, cx).into_any_element()),
                "xray" => Some(self.xray_block(&view, &repaint, cx).into_any_element()),
                "chain" => Some(self.chain_block(&view, &repaint, cx).into_any_element()),
                "extra-core" => Some(self.extra_block(&view, &repaint, cx).into_any_element()),
                _ => None,
            }
        };

        let params_row = div()
            .id("field-params")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(Label::new("Params")))
                    .child(advanced_toggle_button(
                        "Back to guided",
                        &view,
                        &toggle_form,
                    )),
            )
            .child(Textarea::new(&self.params).h(px(96.)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Button::new("params-format").label("Format JSON").on_click(
                        move |_, window, cx| {
                            format_view.update(cx, |view, cx| {
                                view.format_dialog_params(&format_params, window, cx);
                            });
                        },
                    ))
                    .child(
                        Button::new("params-template")
                            .label("Load protocol template")
                            .on_click(move |_, window, cx| {
                                template_view.update(cx, |view, cx| {
                                    view.template_dialog_params(
                                        &template_params,
                                        &template_form,
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(Button::new("params-validate").label("Validate").on_click(
                        move |_, window, cx| {
                            validate_view.update(cx, |view, cx| {
                                let protocol = validate_form.protocol_text(cx);
                                view.validate_dialog_params(
                                    &validate_params,
                                    &protocol,
                                    window,
                                    cx,
                                );
                            });
                        },
                    )),
            );

        let mut form = v_form().small();

        let mut common_head = div().flex().flex_row().items_center().gap_2().child(
            div()
                .flex_1()
                .id("common-caption")
                .test_support()
                .child(Label::new("Common")),
        );
        if has_advanced_dialog(&protocol_text) {
            common_head = common_head.child(advanced_settings_button(&view, &toggle_form));
        }
        let mut common = div().flex().flex_col().gap_1().child(common_head).child(
            field().label("Name *".to_string()).child(
                div()
                    .id("field-name-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| {
                            Tooltip::new(
                                "Display name for this row (cards and toasts name it)".to_string(),
                            )
                        })
                        .into()
                    })
                    .child(
                        div()
                            .id("field-name")
                            .test_support()
                            .child(Input::new(&self.name)),
                    ),
            ),
        );

        if self.protocol_locked {
            let locked = self.locked_protocol.clone();
            common = common.child(
                field().label("Protocol".to_string()).child(
                    div()
                        .id("protocol-lock")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new("Type is locked after creation".to_string()))
                                .into()
                        })
                        .child(Label::new(format!("{locked} (locked)"))),
                ),
            );
        } else {
            let select = self.protocol_select.clone();
            common = common.child(
                field().label("Protocol".to_string()).child(
                    div()
                        .id("protocol-select-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new("Protocol (locked after creation)".to_string()))
                                .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&select)
                                .id("protocol-select")
                                .small()
                                .accessibility_label("Protocol"),
                        ),
                ),
            );
        }

        let has_page = guided_panels.is_some();
        let camouflage = if has_page && has_network_panel(&protocol_text) {
            self.s1_camouflage_section(cx)
        } else {
            None
        };
        let show_right =
            (has_page && tls_panel_plan(&protocol_text).is_some()) || camouflage.is_some();
        if expanded {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("common-section").test_support().child(common)),
            );
            form = form.child(
                field()
                    .label_indent(false)
                    .child(Collapsible::new().open(expanded).content(params_row)),
            );
        } else {
            if !virtual_row {
                common = common.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().child(
                                field().label("Server *").child(
                                    div()
                                        .id("field-guided-server-tip")
                                        .test_support()
                                        .tooltip(move |_, cx| {
                                            cx.new(|_| {
                                                Tooltip::new(
                                                    "Upstream server address (IP or hostname)"
                                                        .to_string(),
                                                )
                                            })
                                            .into()
                                        })
                                        .child(
                                            div()
                                                .id("field-guided-server")
                                                .test_support()
                                                .child(Input::new(&self.guided_server)),
                                        ),
                                ),
                            ),
                        )
                        .child(
                            div().flex_1().min_w_0().child(
                                field().label("Server port *").child(
                                    div()
                                        .id("field-guided-port-tip")
                                        .test_support()
                                        .tooltip(move |_, cx| {
                                            cx.new(|_| {
                                                Tooltip::new("Upstream server port".to_string())
                                            })
                                            .into()
                                        })
                                        .child(
                                            div()
                                                .id("field-guided-port")
                                                .test_support()
                                                .child(Input::new(&self.guided_port)),
                                        ),
                                ),
                            ),
                        ),
                );
            }

            if guided.is_some_and(|guided| guided.key.is_some())
                && !block_owns_identity(&protocol_text)
            {
                let identity_presence =
                    secret_state_caption(self.guided_identity.read(cx).value().as_ref())
                        .to_string();
                common = common.child(
                    field()
                        .label(format!("{} *", guided_identity_label(&protocol_text)))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .id("field-guided-identity")
                                        .test_support()
                                        .child(Input::new(&self.guided_identity).mask_toggle()),
                                )
                                .child(
                                    div()
                                        .id("field-guided-identity-state")
                                        .test_support()
                                        .text_sm()
                                        .child(identity_presence),
                                ),
                        ),
                );
            }
            let mut hint_row = div().flex().flex_row().items_center().gap_2();
            if let Some(hint) = guided_hint_line(&protocol_text) {
                hint_row =
                    hint_row.child(div().flex_1().id("guided-hint").test_support().child(hint));
            } else {
                hint_row = hint_row.child(div().flex_1());
            }
            common = common.child(hint_row.child(advanced_toggle_button(
                "Advanced (raw JSON)",
                &view,
                &toggle_form,
            )));
            let mut left_col = div().flex().flex_col().gap_2().flex_1().min_w_0().child(
                field()
                    .label_indent(false)
                    .child(div().id("common-section").test_support().child(common)),
            );
            if let Some(panels) = guided_panels {
                left_col = left_col.child(field().label_indent(false).child(panels));
            }
            if show_right {
                let mut right_col = div().flex().flex_col().gap_2().flex_1().min_w_0();
                if has_page && tls_panel_plan(&protocol_text).is_some() {
                    right_col = right_col.child(self.s2_security_section(cx));
                }
                if let Some(camouflage) = camouflage {
                    right_col = right_col.child(camouflage);
                }
                form = form.child(
                    field().label_indent(false).child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(left_col)
                            .child(right_col),
                    ),
                );
            } else {
                form = form.child(field().label_indent(false).child(left_col));
            }
        }
        form = form.child(dialog_field("field-group", "Group", self.group));
        form = form.child(
            field().label_indent(false).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-bind",
                        "Bind address",
                        self.bind,
                    )))
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-port",
                        "Port",
                        self.port,
                    ))),
            ),
        );
        if let Some(budget) = port_budget {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("port-budget")
                        .test_support()
                        .text_sm()
                        .child(budget),
                ),
            );
        }

        if show_test_url {
            form = form.child(dialog_field(
                "field-test-url",
                "Test URL (blank = global)",
                self.test_url,
            ));
        }
        if let Some(hint) = lan_hint_for_bind(&bind) {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("lan-hint").test_support().child(hint)),
            );
        }
        if let Some(warning) = collision {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("bind-collision-warning")
                        .test_support()
                        .child(warning),
                ),
            );
        }
        if let Some((field_name, message)) = form_error {
            form = form.child(
                field().label_indent(false).child(
                    div()
                        .id("form-field-error")
                        .test_support()
                        .child(format!("{field_name}: {message}")),
                ),
            );
        }
        form
    }

    pub(crate) fn focus_field(&self, field: &str, window: &mut Window, cx: &mut App) {
        if field == "protocol" {
            if !self.protocol_locked {
                self.protocol_select.update(cx, |state, cx| {
                    state.focus(window, cx);
                });
            }
            return;
        }

        let target: Option<AnyInputState> = match field {
            "name" => Some(AnyInputState::from(self.name.clone())),
            "params" => Some(AnyInputState::from(self.params.clone())),
            "group" => Some(AnyInputState::from(self.group.clone())),
            "bind_ip" => Some(AnyInputState::from(self.bind.clone())),
            "port" => Some(AnyInputState::from(self.port.clone())),
            "test_url" => Some(AnyInputState::from(self.test_url.clone())),
            _ => None,
        };
        if let Some(state) = target {
            let handle = state.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    pub(crate) fn as_new(&self, cx: &App) -> NewConfig {
        NewConfig {
            name: self.name.read(cx).value().to_string(),
            protocol: self.protocol_text(cx),
            params: self.effective_params(cx),
            group: self.group.read(cx).value().to_string(),
            bind_ip: self.bind.read(cx).value().to_string(),
            port: self.port.read(cx).value().to_string(),
        }
    }

    pub(crate) fn effective_params(&self, cx: &App) -> String {
        let params = self.params.read(cx).value().to_string();
        let protocol = self.protocol_text(cx);
        let identity = guided_identity_fields(&protocol)
            .and_then(|guided| guided.key.map(|key| (key, guided.read_keys)));
        let params = fold_guided_into_params(
            &params,
            self.guided_server.read(cx).value().as_ref(),
            self.guided_port.read(cx).value().as_ref(),
            identity,
            self.guided_identity.read(cx).value().as_ref(),
        );
        match protocol.as_str() {
            "vless" => {
                let (uuid, flow, packet_encoding) = self.vless_inner_values(cx);
                let params = fold_vless_inner_into_params(&params, &uuid, &flow, &packet_encoding);
                let params = fold_transport_into_params(&params, &self.transport_fold(cx));
                let params = fold_tls_optional_into_params(&params, &self.tls_optional_fold(cx));
                let params = if flow == "xtls-rprx-vision" {
                    params
                } else {
                    fold_mux_into_params(&params, &self.mux_fold(cx))
                };
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "vmess" => {
                let (uuid, aid, cipher, packet_encoding) = self.vmess_inner_values(cx);
                let params =
                    fold_vmess_inner_into_params(&params, &uuid, &aid, &cipher, &packet_encoding);
                let params = fold_transport_into_params(&params, &self.transport_fold(cx));
                let params = fold_tls_vmess_into_params(&params, &self.tls_optional_fold(cx));
                let params = fold_mux_into_params(&params, &self.mux_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "trojan" => {
                let password = self.trojan_password.read(cx).value().to_string();
                let params = fold_trojan_inner_into_params(&params, &password);
                let params = fold_transport_into_params(&params, &self.transport_fold(cx));
                let params = fold_tls_optional_into_params(&params, &self.tls_optional_fold(cx));
                let params = fold_mux_into_params(&params, &self.mux_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "shadowsocks" => {
                let (method, password, plugin, plugin_opts, uot) = self.ss_inner_values(cx);
                let params = fold_shadowsocks_inner_into_params(
                    &params,
                    &method,
                    &password,
                    &plugin,
                    &plugin_opts,
                    uot,
                );
                let params = fold_mux_into_params(&params, &self.mux_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "wireguard" => {
                let params =
                    fold_wireguard_into_params(&params, &self.wireguard_fold(cx, "wireguard"));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "amneziawg" => {
                let params =
                    fold_wireguard_into_params(&params, &self.wireguard_fold(cx, "amneziawg"));
                let params = fold_amneziawg_into_params(&params, &self.amnezia_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "hysteria" | "hysteria2" => {
                let params = fold_hysteria_into_params(
                    &params,
                    &self.hysteria_fold(cx),
                    protocol == "hysteria2",
                );
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "tuic" => {
                let params = fold_tuic_into_params(&params, &self.tuic_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }

            "socks" => {
                let params = fold_socks_into_params(&params, &self.socks_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "http" | "https" => {
                let params = fold_http_into_params(&params, &self.http_fold(cx));
                let params = fold_tls_optional_into_params(&params, &self.tls_optional_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "anytls" => {
                let params = fold_anytls_into_params(&params, &self.anytls_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "naive" => {
                let params = fold_naive_into_params(&params, &self.naive_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "ssh" => {
                let params = fold_ssh_into_params(&params, &self.ssh_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }

            "snell" => {
                let params = fold_snell_into_params(&params, &self.snell_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "shadowtls" => {
                let params = fold_shadowtls_into_params(&params, &self.shadowtls_fold(cx));
                let params = fold_tls_forced_into_params(&params, &self.tls_forced_fold(cx));
                fold_dial_into_params(&params, &self.dial_fold(cx))
            }
            "xray" => fold_xray_into_params(&params, &self.xray_fold(cx)),
            "chain" => fold_chain_nodes_into_params(&params, &self.chain_hops.borrow()),
            "extra-core" => fold_extra_into_params(&params, &self.extra_fold(cx)),
            _ => params,
        }
    }

    pub(crate) fn panel_value(
        select: &Entity<SelectState<PanelChoiceList>>,
        cx: &App,
        default: &str,
    ) -> String {
        select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    pub(crate) fn vless_inner_values(&self, cx: &App) -> (String, String, String) {
        (
            self.vless_uuid.read(cx).value().to_string(),
            Self::panel_value(&self.vless_flow, cx, ""),
            Self::panel_value(&self.vless_packet_encoding, cx, ""),
        )
    }

    pub(crate) fn vmess_inner_values(&self, cx: &App) -> (String, String, String, String) {
        (
            self.vmess_uuid.read(cx).value().to_string(),
            self.vmess_aid.read(cx).value().to_string(),
            Self::panel_value(&self.vmess_security, cx, "auto"),
            Self::panel_value(&self.vmess_packet_encoding, cx, ""),
        )
    }

    pub(crate) fn ss_inner_values(&self, cx: &App) -> (String, String, String, String, bool) {
        (
            Self::panel_value(&self.ss_method, cx, ""),
            self.ss_password.read(cx).value().to_string(),
            Self::panel_value(&self.ss_plugin, cx, ""),
            self.ss_plugin_opts.read(cx).value().to_string(),
            *self.ss_uot.borrow(),
        )
    }

    pub(crate) fn wireguard_fold(&self, cx: &App, page: &str) -> WireguardFold {
        let awg = page == "amneziawg";
        WireguardFold {
            private_key: if awg {
                self.awg_private.read(cx).value().to_string()
            } else {
                self.wg_private.read(cx).value().to_string()
            },
            public_key: if awg {
                self.awg_public.read(cx).value().to_string()
            } else {
                self.wg_public.read(cx).value().to_string()
            },
            pre_shared_key: if awg {
                self.awg_preshared.read(cx).value().to_string()
            } else {
                self.wg_preshared.read(cx).value().to_string()
            },
            reserved: if awg {
                self.awg_reserved.read(cx).value().to_string()
            } else {
                self.wg_reserved.read(cx).value().to_string()
            },
            persistent_keepalive: if awg {
                self.awg_keepalive.read(cx).value().to_string()
            } else {
                self.wg_keepalive.read(cx).value().to_string()
            },
            local_addr: if awg {
                self.awg_local_addr.read(cx).value().to_string()
            } else {
                self.wg_local_addr.read(cx).value().to_string()
            },
            dns: if awg {
                self.awg_dns.read(cx).value().to_string()
            } else {
                self.wg_dns.read(cx).value().to_string()
            },
            mtu: if awg {
                self.awg_mtu.read(cx).value().to_string()
            } else {
                self.wg_mtu.read(cx).value().to_string()
            },
            workers: if awg {
                self.awg_workers.read(cx).value().to_string()
            } else {
                self.wg_workers.read(cx).value().to_string()
            },
            sys_ifc: if awg {
                *self.awg_sys_ifc.borrow()
            } else {
                *self.wg_sys_ifc.borrow()
            },
        }
    }

    pub(crate) fn amnezia_fold(&self, cx: &App) -> AmneziaFold {
        AmneziaFold {
            enable_amnezia: *self.awg_enable.borrow(),
            version: Self::panel_value(&self.awg_version, cx, ""),
            jc: self.awg_jc.read(cx).value().to_string(),
            jmin: self.awg_jmin.read(cx).value().to_string(),
            jmax: self.awg_jmax.read(cx).value().to_string(),
            s1: self.awg_s1.read(cx).value().to_string(),
            s2: self.awg_s2.read(cx).value().to_string(),
            s3: self.awg_s3.read(cx).value().to_string(),
            s4: self.awg_s4.read(cx).value().to_string(),
            h1: self.awg_h1.read(cx).value().to_string(),
            h2: self.awg_h2.read(cx).value().to_string(),
            h3: self.awg_h3.read(cx).value().to_string(),
            h4: self.awg_h4.read(cx).value().to_string(),
            i1: self.awg_i1.read(cx).value().to_string(),
            i2: self.awg_i2.read(cx).value().to_string(),
            i3: self.awg_i3.read(cx).value().to_string(),
            i4: self.awg_i4.read(cx).value().to_string(),
            i5: self.awg_i5.read(cx).value().to_string(),
            header_protection_key: self.awg_hpk.read(cx).value().to_string(),
            content_padding_addition: self.awg_pad.read(cx).value().to_string(),
            rekey_after_time: self.awg_rekey_after.read(cx).value().to_string(),
            rekey_timeout: self.awg_rekey_timeout.read(cx).value().to_string(),
            reject_after_time: self.awg_reject_after.read(cx).value().to_string(),
            keepalive_timeout: self.awg_keepalive_timeout.read(cx).value().to_string(),
            max_handshake_attempts: self.awg_max_handshake.read(cx).value().to_string(),
            random_trailers: *self.awg_trailers.borrow(),
            disable_cookies: *self.awg_cookies.borrow(),
        }
    }

    pub(crate) fn hysteria_fold(&self, cx: &App) -> HysteriaFold {
        HysteriaFold {
            server_ports: self.hy_server_ports.read(cx).value().to_string(),
            hop_interval: self.hy_hop_interval.read(cx).value().to_string(),
            up: self.hy_up.read(cx).value().to_string(),
            down: self.hy_down.read(cx).value().to_string(),
            obfs: self.hy_obfs.read(cx).value().to_string(),
            auth_type: Self::panel_value(&self.hy_auth_type, cx, ""),
            auth: self.hy_auth.read(cx).value().to_string(),
            recv_window_conn: self.hy_recv_conn.read(cx).value().to_string(),
            recv_window: self.hy_recv.read(cx).value().to_string(),
            disable_mtu_discovery: *self.hy_mtu.borrow(),
            password: self.hy_password.read(cx).value().to_string(),
        }
    }

    pub(crate) fn tuic_fold(&self, cx: &App) -> TuicFold {
        TuicFold {
            uuid: self.tuic_uuid.read(cx).value().to_string(),
            password: self.tuic_password.read(cx).value().to_string(),
            congestion_control: Self::panel_value(&self.tuic_cc, cx, ""),
            udp_relay_mode: Self::panel_value(&self.tuic_relay, cx, ""),
            udp_over_stream: *self.tuic_uot.borrow(),
            zero_rtt_handshake: *self.tuic_zero_rtt.borrow(),
            heartbeat: self.tuic_heartbeat.read(cx).value().to_string(),
        }
    }

    pub(crate) fn socks_fold(&self, cx: &App) -> SocksFold {
        SocksFold {
            version: Self::panel_value(&self.socks_version, cx, ""),
            username: self.socks_username.read(cx).value().to_string(),
            password: self.socks_password.read(cx).value().to_string(),
            network: Self::panel_value(&self.socks_network, cx, "tcp"),
            udp_over_tcp: *self.socks_uot.borrow(),
        }
    }

    pub(crate) fn http_fold(&self, cx: &App) -> HttpFold {
        HttpFold {
            username: self.http_username.read(cx).value().to_string(),
            password: self.http_password.read(cx).value().to_string(),
            path: self.http_path.read(cx).value().to_string(),
            headers: self.http_headers.read(cx).value().to_string(),
        }
    }

    pub(crate) fn anytls_fold(&self, cx: &App) -> AnytlsFold {
        AnytlsFold {
            password: self.anytls_password.read(cx).value().to_string(),
            interval: self.anytls_interval.read(cx).value().to_string(),
            timeout: self.anytls_timeout.read(cx).value().to_string(),
            min: self.anytls_min.read(cx).value().to_string(),
        }
    }

    pub(crate) fn naive_fold(&self, cx: &App) -> NaiveFold {
        NaiveFold {
            username: self.naive_username.read(cx).value().to_string(),
            password: self.naive_password.read(cx).value().to_string(),
            udp_over_tcp: *self.naive_uot.borrow(),
            quic: *self.naive_quic.borrow(),
            congestion_control: Self::panel_value(&self.naive_cc, cx, ""),
        }
    }

    pub(crate) fn ssh_fold(&self, cx: &App) -> SshFold {
        SshFold {
            user: self.ssh_user.read(cx).value().to_string(),
            password: self.ssh_password.read(cx).value().to_string(),
            private_key: self.ssh_private_key.read(cx).value().to_string(),
            private_key_path: self.ssh_key_path.read(cx).value().to_string(),
            private_key_passphrase: self.ssh_key_pass.read(cx).value().to_string(),
            host_key: self.ssh_host_key.read(cx).value().to_string(),
            host_key_algorithms: self.ssh_host_key_algs.read(cx).value().to_string(),
            client_version: self.ssh_client_version.read(cx).value().to_string(),
        }
    }

    pub(crate) fn snell_fold(&self, cx: &App) -> SnellFold {
        SnellFold {
            psk: self.snell_psk.read(cx).value().to_string(),
            version: Self::panel_value(&self.snell_version, cx, ""),
            obfs_mode: self.snell_obfs_mode.read(cx).value().to_string(),
            obfs_host: self.snell_obfs_host.read(cx).value().to_string(),
            userkey: self.snell_userkey.read(cx).value().to_string(),
            reuse: *self.snell_reuse.borrow(),
            network: self.snell_network.read(cx).value().to_string(),
            mode: self.snell_mode.read(cx).value().to_string(),
        }
    }

    pub(crate) fn shadowtls_fold(&self, cx: &App) -> ShadowTlsFold {
        ShadowTlsFold {
            version: Self::panel_value(&self.shadowtls_version, cx, ""),
            password: self.shadowtls_password.read(cx).value().to_string(),
        }
    }

    pub(crate) fn xray_fold(&self, cx: &App) -> XrayFold {
        XrayFold {
            uuid: self.xray_uuid.read(cx).value().to_string(),
            encryption: self.xray_enc.read(cx).value().to_string(),
            flow: Self::panel_value(&self.xray_flow, cx, ""),
            sni: self.xray_sni.read(cx).value().to_string(),
            fp: Self::panel_value(&self.xray_fp, cx, ""),
            alpn: self.xray_alpn.read(cx).value().to_string(),
            pbk: self.xray_pbk.read(cx).value().to_string(),
            sid: self.xray_sid.read(cx).value().to_string(),
            spiderx: self.xray_spiderx.read(cx).value().to_string(),
            network: Self::panel_value(&self.xray_network, cx, ""),
            mode: self.xray_mode.read(cx).value().to_string(),
            mux: Self::panel_value(&self.xray_mux, cx, ""),
        }
    }

    pub(crate) fn extra_fold(&self, cx: &App) -> ExtraFold {
        ExtraFold {
            exe: self.extra_exe.read(cx).value().to_string(),
            args: self.extra_args.read(cx).value().to_string(),
            config: self.extra_config.read(cx).value().to_string(),
        }
    }

    pub(crate) fn tls_forced_fold(&self, cx: &App) -> TlsForcedFold {
        TlsForcedFold {
            sni: self.s2_sni.read(cx).value().to_string(),
            alpn: self.s2_alpn.read(cx).value().to_string(),
            fp: Self::panel_value(&self.s2_fp, cx, ""),
            insecure: *self.s2_insecure.borrow(),
            pbk: self.s2_pbk.read(cx).value().to_string(),
            sid: self.s2_sid.read(cx).value().to_string(),
        }
    }

    pub(crate) fn transport_fold(&self, cx: &App) -> TransportFold {
        TransportFold {
            network: Self::panel_value(&self.s1_network, cx, "tcp"),
            path: self.s1_path.read(cx).value().to_string(),
            host: self.s1_host.read(cx).value().to_string(),
            method: self.s1_method.read(cx).value().to_string(),
            service_name: self.s1_service_name.read(cx).value().to_string(),
        }
    }

    pub(crate) fn tls_optional_fold(&self, cx: &App) -> TlsOptionalFold {
        TlsOptionalFold {
            enable: *self.s2_enable.borrow(),
            sni: self.s2_sni.read(cx).value().to_string(),
            alpn: self.s2_alpn.read(cx).value().to_string(),
            fp: Self::panel_value(&self.s2_fp, cx, ""),
            insecure: *self.s2_insecure.borrow(),
            pbk: self.s2_pbk.read(cx).value().to_string(),
            sid: self.s2_sid.read(cx).value().to_string(),
        }
    }

    pub(crate) fn s3_effective_mode(&self) -> String {
        if *self.s3_brutal.borrow() {
            "on".to_string()
        } else {
            self.s3_mode.borrow().clone()
        }
    }

    pub(crate) fn mux_fold(&self, cx: &App) -> MuxFold {
        MuxFold {
            mode: self.s3_effective_mode(),
            brutal_enable: *self.s3_brutal.borrow(),
            brutal_up_mbps: self.s3_up.read(cx).value().to_string(),
            brutal_down_mbps: self.s3_down.read(cx).value().to_string(),
        }
    }

    pub(crate) fn dial_fold(&self, cx: &App) -> DialFold {
        DialFold {
            reuse_addr: *self.s4_reuse.borrow(),
            tcp_fast_open: *self.s4_fastopen.borrow(),
            udp_fragment: *self.s4_fragment.borrow(),
            tcp_multi_path: *self.s4_multipath.borrow(),
            connect_timeout: self.s4_timeout.read(cx).value().to_string(),
            bind_interface: self.s4_bind.read(cx).value().to_string(),
            inet4_bind_address: self.s4_inet4.read(cx).value().to_string(),
            inet6_bind_address: self.s4_inet6.read(cx).value().to_string(),
        }
    }

    pub(crate) fn expand_merge(&self, window: &mut Window, cx: &mut Context<AppView>) {
        let box_text = self.params.read(cx).value().to_string();
        let protocol = self.protocol_text(cx);
        let identity = guided_identity_fields(&protocol)
            .and_then(|guided| guided.key.map(|key| (key, guided.read_keys)));
        let mut merged = fold_guided_into_params(
            &box_text,
            self.guided_server.read(cx).value().as_ref(),
            self.guided_port.read(cx).value().as_ref(),
            identity,
            self.guided_identity.read(cx).value().as_ref(),
        );

        let page = matches!(
            protocol.as_str(),
            "vless"
                | "vmess"
                | "trojan"
                | "shadowsocks"
                | "wireguard"
                | "amneziawg"
                | "hysteria"
                | "hysteria2"
                | "tuic"
                | "socks"
                | "http"
                | "https"
                | "anytls"
                | "naive"
                | "ssh"
                | "snell"
                | "shadowtls"
                | "xray"
                | "chain"
                | "extra-core"
        );
        if page {
            match protocol.as_str() {
                "vless" => {
                    let (uuid, flow, packet_encoding) = self.vless_inner_values(cx);
                    merged = fold_vless_inner_into_params(&merged, &uuid, &flow, &packet_encoding);
                    merged = fold_transport_into_params(&merged, &self.transport_fold(cx));
                    merged = fold_tls_optional_into_params(&merged, &self.tls_optional_fold(cx));
                    if flow != "xtls-rprx-vision" {
                        merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    }
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "vmess" => {
                    let (uuid, aid, cipher, packet_encoding) = self.vmess_inner_values(cx);
                    merged = fold_vmess_inner_into_params(
                        &merged,
                        &uuid,
                        &aid,
                        &cipher,
                        &packet_encoding,
                    );
                    merged = fold_transport_into_params(&merged, &self.transport_fold(cx));
                    merged = fold_tls_vmess_into_params(&merged, &self.tls_optional_fold(cx));
                    merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "trojan" => {
                    let password = self.trojan_password.read(cx).value().to_string();
                    merged = fold_trojan_inner_into_params(&merged, &password);
                    merged = fold_transport_into_params(&merged, &self.transport_fold(cx));
                    merged = fold_tls_optional_into_params(&merged, &self.tls_optional_fold(cx));
                    merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "shadowsocks" => {
                    let (method, password, plugin, plugin_opts, uot) = self.ss_inner_values(cx);
                    merged = fold_shadowsocks_inner_into_params(
                        &merged,
                        &method,
                        &password,
                        &plugin,
                        &plugin_opts,
                        uot,
                    );
                    merged = fold_mux_into_params(&merged, &self.mux_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "wireguard" => {
                    merged =
                        fold_wireguard_into_params(&merged, &self.wireguard_fold(cx, "wireguard"));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "amneziawg" => {
                    merged =
                        fold_wireguard_into_params(&merged, &self.wireguard_fold(cx, "amneziawg"));
                    merged = fold_amneziawg_into_params(&merged, &self.amnezia_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "hysteria" | "hysteria2" => {
                    merged = fold_hysteria_into_params(
                        &merged,
                        &self.hysteria_fold(cx),
                        protocol == "hysteria2",
                    );
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "tuic" => {
                    merged = fold_tuic_into_params(&merged, &self.tuic_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }

                "socks" => {
                    merged = fold_socks_into_params(&merged, &self.socks_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "http" | "https" => {
                    merged = fold_http_into_params(&merged, &self.http_fold(cx));
                    merged = fold_tls_optional_into_params(&merged, &self.tls_optional_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "anytls" => {
                    merged = fold_anytls_into_params(&merged, &self.anytls_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "naive" => {
                    merged = fold_naive_into_params(&merged, &self.naive_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "ssh" => {
                    merged = fold_ssh_into_params(&merged, &self.ssh_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }

                "snell" => {
                    merged = fold_snell_into_params(&merged, &self.snell_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "shadowtls" => {
                    merged = fold_shadowtls_into_params(&merged, &self.shadowtls_fold(cx));
                    merged = fold_tls_forced_into_params(&merged, &self.tls_forced_fold(cx));
                    merged = fold_dial_into_params(&merged, &self.dial_fold(cx));
                }
                "xray" => {
                    merged = fold_xray_into_params(&merged, &self.xray_fold(cx));
                }
                "chain" => {
                    merged = fold_chain_nodes_into_params(&merged, &self.chain_hops.borrow());
                }
                "extra-core" => {
                    merged = fold_extra_into_params(&merged, &self.extra_fold(cx));
                }
                _ => {}
            }
        }
        let parseable = merged.trim().is_empty()
            || serde_json::from_str::<serde_json::Value>(&merged)
                .ok()
                .is_some_and(|value| value.is_object());
        if merged != box_text {
            let next = merged.clone();
            self.params.update(cx, |state, cx| {
                state.set_value(next, window, cx);
            });
        }
        if parseable {
            for entity in [
                &self.guided_server,
                &self.guided_port,
                &self.guided_identity,
            ] {
                entity.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            }
            if page {
                self.clear_panels(window, cx, &protocol);
            }
        }
    }

    pub(crate) fn clear_panels(
        &self,
        window: &mut Window,
        cx: &mut Context<AppView>,
        protocol: &str,
    ) {
        let mut texts: Vec<&Entity<InputState>> = vec![
            &self.s4_timeout,
            &self.s4_bind,
            &self.s4_inet4,
            &self.s4_inet6,
        ];
        let mut selects: Vec<(&Entity<SelectState<PanelChoiceList>>, &'static str)> = vec![];
        let mut cells: Vec<&Rc<RefCell<bool>>> = vec![
            &self.s4_reuse,
            &self.s4_fastopen,
            &self.s4_fragment,
            &self.s4_multipath,
        ];
        let mux_page = matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks");
        if mux_page {
            texts.extend([&self.s3_up, &self.s3_down]);
            cells.push(&self.s3_brutal);
        }
        if protocol != "shadowsocks" {
            let forced_tls = matches!(
                protocol,
                "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls"
            );
            let inner_only = matches!(
                protocol,
                "socks" | "ssh" | "snell" | "xray" | "chain" | "extra-core"
            );
            let no_transport = matches!(protocol, "http" | "https");
            if !forced_tls && !inner_only && !no_transport {
                texts.extend([
                    &self.s1_path,
                    &self.s1_host,
                    &self.s1_method,
                    &self.s1_service_name,
                ]);
                selects.extend([(&self.s1_network, "tcp")]);
                cells.extend([&self.s2_enable]);
            }
            if !inner_only {
                if no_transport {
                    cells.extend([&self.s2_enable]);
                }
                texts.extend([&self.s2_sni, &self.s2_alpn, &self.s2_pbk, &self.s2_sid]);
                selects.extend([(&self.s2_fp, "")]);
                cells.extend([&self.s2_insecure]);
            }
        }
        match protocol {
            "vless" => {
                texts.push(&self.vless_uuid);
                selects.extend([(&self.vless_flow, ""), (&self.vless_packet_encoding, "")]);
            }
            "vmess" => {
                texts.extend([&self.vmess_uuid, &self.vmess_aid]);
                selects.extend([
                    (&self.vmess_security, "auto"),
                    (&self.vmess_packet_encoding, ""),
                ]);
            }
            "trojan" => {
                texts.push(&self.trojan_password);
            }
            "shadowsocks" => {
                texts.extend([&self.ss_password, &self.ss_plugin_opts]);
                selects.extend([(&self.ss_method, ""), (&self.ss_plugin, "")]);
                cells.push(&self.ss_uot);
            }
            "hysteria" | "hysteria2" => {
                texts.extend([
                    &self.hy_server_ports,
                    &self.hy_hop_interval,
                    &self.hy_up,
                    &self.hy_down,
                    &self.hy_obfs,
                    &self.hy_auth,
                    &self.hy_recv_conn,
                    &self.hy_recv,
                    &self.hy_password,
                ]);
                selects.extend([(&self.hy_auth_type, "")]);
                cells.push(&self.hy_mtu);
            }
            "tuic" => {
                texts.extend([&self.tuic_uuid, &self.tuic_password, &self.tuic_heartbeat]);
                selects.extend([(&self.tuic_cc, ""), (&self.tuic_relay, "")]);
                cells.extend([&self.tuic_uot, &self.tuic_zero_rtt]);
            }

            "socks" => {
                texts.extend([&self.socks_username, &self.socks_password]);
                selects.extend([(&self.socks_version, ""), (&self.socks_network, "tcp")]);
                cells.push(&self.socks_uot);
            }
            "http" | "https" => {
                texts.extend([
                    &self.http_username,
                    &self.http_password,
                    &self.http_path,
                    &self.http_headers,
                ]);
            }
            "anytls" => {
                texts.extend([
                    &self.anytls_password,
                    &self.anytls_interval,
                    &self.anytls_timeout,
                    &self.anytls_min,
                ]);
            }
            "naive" => {
                texts.extend([&self.naive_username, &self.naive_password]);
                selects.extend([(&self.naive_cc, "")]);
                cells.extend([&self.naive_uot, &self.naive_quic]);
            }
            "ssh" => {
                texts.extend([
                    &self.ssh_user,
                    &self.ssh_password,
                    &self.ssh_key_path,
                    &self.ssh_key_pass,
                    &self.ssh_host_key,
                    &self.ssh_host_key_algs,
                    &self.ssh_client_version,
                ]);

                self.ssh_private_key.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            }

            "snell" => {
                texts.extend([
                    &self.snell_psk,
                    &self.snell_obfs_mode,
                    &self.snell_obfs_host,
                    &self.snell_userkey,
                    &self.snell_network,
                    &self.snell_mode,
                ]);
                selects.extend([(&self.snell_version, "")]);
                cells.push(&self.snell_reuse);
            }
            "shadowtls" => {
                texts.push(&self.shadowtls_password);
                selects.extend([(&self.shadowtls_version, "")]);
            }
            "xray" => {
                texts.extend([
                    &self.xray_uuid,
                    &self.xray_enc,
                    &self.xray_sni,
                    &self.xray_alpn,
                    &self.xray_pbk,
                    &self.xray_sid,
                    &self.xray_spiderx,
                    &self.xray_mode,
                ]);
                selects.extend([
                    (&self.xray_flow, ""),
                    (&self.xray_fp, ""),
                    (&self.xray_network, ""),
                    (&self.xray_mux, ""),
                ]);
            }
            "chain" => {}
            "extra-core" => {
                texts.extend([&self.extra_exe, &self.extra_args]);

                self.extra_config.update(cx, |state, cx| {
                    state.set_value(String::new(), window, cx);
                });
            }
            "wireguard" => {
                texts.extend([
                    &self.wg_private,
                    &self.wg_public,
                    &self.wg_preshared,
                    &self.wg_reserved,
                    &self.wg_keepalive,
                    &self.wg_local_addr,
                    &self.wg_dns,
                    &self.wg_mtu,
                    &self.wg_workers,
                ]);
                cells.push(&self.wg_sys_ifc);
            }
            "amneziawg" => {
                texts.extend([
                    &self.awg_private,
                    &self.awg_public,
                    &self.awg_preshared,
                    &self.awg_reserved,
                    &self.awg_keepalive,
                    &self.awg_local_addr,
                    &self.awg_dns,
                    &self.awg_mtu,
                    &self.awg_workers,
                    &self.awg_jc,
                    &self.awg_jmin,
                    &self.awg_jmax,
                    &self.awg_s1,
                    &self.awg_s2,
                    &self.awg_s3,
                    &self.awg_s4,
                    &self.awg_h1,
                    &self.awg_h2,
                    &self.awg_h3,
                    &self.awg_h4,
                    &self.awg_i1,
                    &self.awg_i2,
                    &self.awg_i3,
                    &self.awg_i4,
                    &self.awg_i5,
                    &self.awg_hpk,
                    &self.awg_pad,
                    &self.awg_rekey_after,
                    &self.awg_rekey_timeout,
                    &self.awg_reject_after,
                    &self.awg_keepalive_timeout,
                    &self.awg_max_handshake,
                ]);
                selects.extend([(&self.awg_version, "")]);
                cells.extend([
                    &self.awg_sys_ifc,
                    &self.awg_enable,
                    &self.awg_trailers,
                    &self.awg_cookies,
                ]);
            }
            _ => {}
        }
        for entity in texts {
            entity.update(cx, |state, cx| {
                state.set_value(String::new(), window, cx);
            });
        }
        for (entity, default) in selects {
            let next = default.to_string();
            entity.update(cx, |state, cx| {
                state.set_selected_value(&next, window, cx);
            });
        }
        for cell in cells {
            *cell.borrow_mut() = false;
        }
        if mux_page {
            *self.s3_mode.borrow_mut() = String::new();
        }
    }

    pub(crate) fn collapse_echo(&self, window: &mut Window, cx: &mut Context<AppView>) {
        let box_text = self.params.read(cx).value().to_string();
        let protocol = self.protocol_text(cx);
        let read_keys: &[&str] = guided_identity_fields(&protocol)
            .map(|guided| guided.read_keys)
            .unwrap_or(&[]);
        for (entity, value) in [
            (
                &self.guided_server,
                guided_prefill_value(&box_text, &["server", "address"]),
            ),
            (
                &self.guided_port,
                guided_prefill_value(&box_text, &["server_port", "port"]),
            ),
            (
                &self.guided_identity,
                guided_prefill_value(&box_text, read_keys),
            ),
        ] {
            let blank = entity.read(cx).value().to_string().trim().is_empty();
            if blank && !value.is_empty() {
                let next = value.clone();
                entity.update(cx, |state, cx| {
                    state.set_value(next, window, cx);
                });
            }
        }
        if matches!(
            protocol.as_str(),
            "vless"
                | "vmess"
                | "trojan"
                | "shadowsocks"
                | "wireguard"
                | "amneziawg"
                | "hysteria"
                | "hysteria2"
                | "tuic"
                | "socks"
                | "http"
                | "https"
                | "anytls"
                | "naive"
                | "ssh"
                | "snell"
                | "shadowtls"
                | "xray"
                | "chain"
                | "extra-core"
        ) {
            self.echo_panels(window, cx, &box_text, &protocol);
        }
    }

    pub(crate) fn echo_panels(
        &self,
        window: &mut Window,
        cx: &mut Context<AppView>,
        box_text: &str,
        protocol: &str,
    ) {
        let full_panels = matches!(protocol, "vless" | "vmess" | "trojan");
        let mux_page = matches!(protocol, "vless" | "vmess" | "trojan" | "shadowsocks");
        let mut shared_texts: Vec<(&Entity<InputState>, &[&str])> = vec![
            (&self.s4_timeout, &["connect_timeout"][..]),
            (&self.s4_bind, &["bind_interface"][..]),
            (&self.s4_inet4, &["inet4_bind_address"][..]),
            (&self.s4_inet6, &["inet6_bind_address"][..]),
        ];

        let forced_tls = matches!(
            protocol,
            "hysteria" | "hysteria2" | "tuic" | "anytls" | "naive" | "shadowtls"
        );
        let optional_tls = matches!(protocol, "http" | "https");
        if full_panels {
            shared_texts.extend([
                (&self.s1_path, &["path"][..]),
                (&self.s1_host, &["host"][..]),
                (&self.s1_method, &["method"][..]),
                (&self.s1_service_name, &["service_name", "serviceName"][..]),
            ]);
        }
        if full_panels || forced_tls || optional_tls {
            shared_texts.extend([
                (&self.s2_sni, &["sni", "peer"][..]),
                (&self.s2_alpn, &["alpn"][..]),
                (&self.s2_pbk, &["pbk"][..]),
                (&self.s2_sid, &["sid"][..]),
            ]);
        }
        if mux_page {
            shared_texts.extend([
                (&self.s3_up, &["brutal_up_mbps", "brutal_up"][..]),
                (&self.s3_down, &["brutal_down_mbps", "brutal_down"][..]),
            ]);
        }
        for (entity, keys) in shared_texts {
            let value = guided_prefill_value(box_text, keys);
            let blank = entity.read(cx).value().to_string().trim().is_empty();
            if blank && !value.is_empty() {
                entity.update(cx, |state, cx| {
                    state.set_value(value.clone(), window, cx);
                });
            }
        }

        let inner_texts: &[(&Entity<InputState>, &[&str])] = match protocol {
            "vless" => &[(&self.vless_uuid, &["uuid", "id"][..])],
            "vmess" => &[
                (&self.vmess_uuid, &["uuid", "id"][..]),
                (&self.vmess_aid, &["alter_id", "aid"][..]),
            ],
            "trojan" => &[(&self.trojan_password, &["password"][..])],
            "shadowsocks" => &[
                (&self.ss_password, &["password"][..]),
                (
                    &self.ss_plugin_opts,
                    &["plugin_opts", "plugin-opts", "pluginOpts"][..],
                ),
            ],

            "hysteria" | "hysteria2" => &[
                (&self.hy_server_ports, &["server_ports"][..]),
                (&self.hy_hop_interval, &["hop_interval"][..]),
                (&self.hy_up, &["up", "up_mbps", "upmbps"][..]),
                (&self.hy_down, &["down", "down_mbps", "downmbps"][..]),
                (&self.hy_obfs, &["obfs"][..]),
                (&self.hy_auth, &["auth"][..]),
                (&self.hy_recv_conn, &["recv_window_conn"][..]),
                (&self.hy_recv, &["recv_window"][..]),
                (&self.hy_password, &["password"][..]),
            ],
            "tuic" => &[
                (&self.tuic_uuid, &["uuid", "id"][..]),
                (&self.tuic_password, &["password"][..]),
                (&self.tuic_heartbeat, &["heartbeat"][..]),
            ],

            "socks" => &[
                (&self.socks_username, &["username"][..]),
                (&self.socks_password, &["password"][..]),
            ],
            "http" | "https" => &[
                (&self.http_username, &["username"][..]),
                (&self.http_password, &["password"][..]),
                (&self.http_path, &["path"][..]),
                (&self.http_headers, &["headers"][..]),
            ],
            "anytls" => &[
                (&self.anytls_password, &["password"][..]),
                (
                    &self.anytls_interval,
                    &["idle_session_check_interval", "interval"][..],
                ),
                (
                    &self.anytls_timeout,
                    &["idle_session_timeout", "timeout"][..],
                ),
                (&self.anytls_min, &["min_idle_session", "min"][..]),
            ],
            "naive" => &[
                (&self.naive_username, &["username"][..]),
                (&self.naive_password, &["password"][..]),
            ],
            "ssh" => &[
                (&self.ssh_user, &["user", "username"][..]),
                (&self.ssh_password, &["password"][..]),
                (&self.ssh_key_path, &["private_key_path"][..]),
                (
                    &self.ssh_key_pass,
                    &["private_key_passphrase", "private_key_pass"][..],
                ),
                (&self.ssh_client_version, &["client_version"][..]),
            ],

            "snell" => &[
                (&self.snell_psk, &["psk"][..]),
                (&self.snell_obfs_mode, &["obfs_mode", "obfs"][..]),
                (&self.snell_obfs_host, &["obfs_host", "obfs-host"][..]),
                (&self.snell_userkey, &["userkey"][..]),
                (&self.snell_network, &["network"][..]),
                (&self.snell_mode, &["mode"][..]),
            ],
            "shadowtls" => &[(&self.shadowtls_password, &["password"][..])],
            "xray" => &[
                (&self.xray_uuid, &["uuid", "id"][..]),
                (&self.xray_enc, &["encryption"][..]),
                (&self.xray_sni, &["sni"][..]),
                (&self.xray_alpn, &["alpn"][..]),
                (&self.xray_pbk, &["pbk"][..]),
                (&self.xray_sid, &["sid"][..]),
                (&self.xray_spiderx, &["spiderx"][..]),
                (&self.xray_mode, &["mode"][..]),
            ],
            "extra-core" => &[
                (&self.extra_exe, &["exe", "binary"][..]),
                (&self.extra_args, &["args"][..]),
            ],

            "wireguard" => &[
                (&self.wg_private, &["private_key", "privatekey"][..]),
                (&self.wg_public, &["public_key", "publickey"][..]),
                (
                    &self.wg_preshared,
                    &["pre_shared_key", "presharedkey", "preshared_key"][..],
                ),
                (&self.wg_reserved, &["reserved"][..]),
                (
                    &self.wg_keepalive,
                    &["persistent_keepalive", "persistent_keepalive_interval"][..],
                ),
                (
                    &self.wg_local_addr,
                    &["address", "local_address", "local_addr"][..],
                ),
                (&self.wg_dns, &["dns"][..]),
                (&self.wg_mtu, &["mtu"][..]),
                (&self.wg_workers, &["workers"][..]),
            ],
            "amneziawg" => &[
                (&self.awg_private, &["private_key", "privatekey"][..]),
                (&self.awg_public, &["public_key", "publickey"][..]),
                (
                    &self.awg_preshared,
                    &["pre_shared_key", "presharedkey", "preshared_key"][..],
                ),
                (&self.awg_reserved, &["reserved"][..]),
                (
                    &self.awg_keepalive,
                    &["persistent_keepalive", "persistent_keepalive_interval"][..],
                ),
                (
                    &self.awg_local_addr,
                    &["address", "local_address", "local_addr"][..],
                ),
                (&self.awg_dns, &["dns"][..]),
                (&self.awg_mtu, &["mtu"][..]),
                (&self.awg_workers, &["workers"][..]),
                (&self.awg_jc, &["jc"][..]),
                (&self.awg_jmin, &["jmin"][..]),
                (&self.awg_jmax, &["jmax"][..]),
                (&self.awg_s1, &["s1"][..]),
                (&self.awg_s2, &["s2"][..]),
                (&self.awg_s3, &["s3"][..]),
                (&self.awg_s4, &["s4"][..]),
                (&self.awg_h1, &["h1"][..]),
                (&self.awg_h2, &["h2"][..]),
                (&self.awg_h3, &["h3"][..]),
                (&self.awg_h4, &["h4"][..]),
                (&self.awg_i1, &["i1"][..]),
                (&self.awg_i2, &["i2"][..]),
                (&self.awg_i3, &["i3"][..]),
                (&self.awg_i4, &["i4"][..]),
                (&self.awg_i5, &["i5"][..]),
                (&self.awg_hpk, &["headerprotectionkey"][..]),
                (&self.awg_pad, &["contentpaddingaddition"][..]),
                (&self.awg_rekey_after, &["rekeyaftertime"][..]),
                (&self.awg_rekey_timeout, &["rekeytimeout"][..]),
                (&self.awg_reject_after, &["rejectaftertime"][..]),
                (&self.awg_keepalive_timeout, &["keepalivetimeout"][..]),
                (&self.awg_max_handshake, &["maxhandshakeattempts"][..]),
            ],
            _ => &[],
        };
        for (entity, keys) in inner_texts {
            let value = guided_prefill_value(box_text, keys);
            let blank = entity.read(cx).value().to_string().trim().is_empty();
            if blank && !value.is_empty() {
                entity.update(cx, |state, cx| {
                    state.set_value(value.clone(), window, cx);
                });
            }
        }

        if protocol == "ssh" {
            for (entity, keys) in [
                (&self.ssh_host_key, &["host_key"][..]),
                (&self.ssh_host_key_algs, &["host_key_algorithms"][..]),
            ] {
                let value = guided_prefill_list(box_text, keys);
                let blank = entity.read(cx).value().to_string().trim().is_empty();
                if blank && !value.is_empty() {
                    entity.update(cx, |state, cx| {
                        state.set_value(value.clone(), window, cx);
                    });
                }
            }
            let key_value = guided_prefill_value(box_text, &["private_key", "privatekey"]);
            let key_blank = self
                .ssh_private_key
                .read(cx)
                .value()
                .to_string()
                .trim()
                .is_empty();
            if key_blank && !key_value.is_empty() {
                self.ssh_private_key.update(cx, |state, cx| {
                    state.set_value(key_value.clone(), window, cx);
                });
            }
        }

        if protocol == "extra-core" {
            let config_value = extra_config_prefill(box_text);
            let config_blank = self
                .extra_config
                .read(cx)
                .value()
                .to_string()
                .trim()
                .is_empty();
            if config_blank && !config_value.is_empty() {
                self.extra_config.update(cx, |state, cx| {
                    state.set_value(config_value.clone(), window, cx);
                });
            }
        }

        if protocol == "chain" {
            *self.chain_hops.borrow_mut() = chain_nodes_of(box_text);
        }

        if protocol == "socks" {
            let value = guided_prefill_value(box_text, &["network"]);
            let current = self
                .socks_network
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == "tcp" && !value.is_empty() && value != "tcp" {
                self.socks_network.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }

        let panel_selects: &[EchoSelect] = if full_panels {
            &[
                (&self.s1_network, &["network"][..], "tcp"),
                (&self.s2_fp, &["fp", "fingerprint"][..], ""),
            ]
        } else if forced_tls || optional_tls {
            &[(&self.s2_fp, &["fp", "fingerprint"][..], "")]
        } else {
            &[]
        };
        for (entity, keys, default) in panel_selects {
            let value = guided_prefill_value(box_text, keys);
            let current = entity
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == *default && !value.is_empty() && value != *default {
                entity.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }

        let inner_selects: &[EchoSelect] = match protocol {
            "vless" => &[
                (&self.vless_flow, &["flow"][..], ""),
                (
                    &self.vless_packet_encoding,
                    &["packet_encoding", "packetEncoding"][..],
                    "",
                ),
            ],
            "vmess" => &[(
                &self.vmess_packet_encoding,
                &["packet_encoding", "packetEncoding"][..],
                "",
            )],
            "shadowsocks" => &[
                (&self.ss_method, &["method"][..], ""),
                (&self.ss_plugin, &["plugin"][..], ""),
            ],

            "amneziawg" => &[(&self.awg_version, &["awg_version", "version"][..], "")],

            "hysteria" | "hysteria2" => &[(&self.hy_auth_type, &["auth_type"][..], "")],
            "tuic" => &[
                (&self.tuic_cc, &["congestion_control"][..], ""),
                (&self.tuic_relay, &["udp_relay_mode"][..], ""),
            ],
            "socks" => &[(&self.socks_version, &["version"][..], "")],
            "naive" => &[(
                &self.naive_cc,
                &["quic_congestion_control", "congestion_control"][..],
                "",
            )],

            "snell" => &[(&self.snell_version, &["version"][..], "")],
            "shadowtls" => &[(&self.shadowtls_version, &["version"][..], "")],
            "xray" => &[
                (&self.xray_flow, &["flow"][..], ""),
                (&self.xray_fp, &["fp", "fingerprint"][..], ""),
                (&self.xray_network, &["network"][..], ""),
                (&self.xray_mux, &["xray_mux"][..], ""),
            ],
            _ => &[],
        };
        for (entity, keys, default) in inner_selects {
            let value = guided_prefill_value(box_text, keys);
            let current = entity
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == *default && !value.is_empty() && value != *default {
                entity.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }

        if protocol == "vmess" {
            let value = vmess_cipher_value(box_text);
            let current = self
                .vmess_security
                .read(cx)
                .selected_value()
                .cloned()
                .unwrap_or_default();
            if current == "auto" && !value.is_empty() && value != "auto" {
                self.vmess_security.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
        }
        let mut shared_cells: Vec<(&Rc<RefCell<bool>>, &[&str])> = vec![
            (&self.s4_reuse, &["reuse_addr"][..]),
            (&self.s4_fastopen, &["tcp_fast_open"][..]),
            (&self.s4_fragment, &["udp_fragment"][..]),
            (&self.s4_multipath, &["tcp_multi_path"][..]),
        ];

        if full_panels || forced_tls || optional_tls {
            shared_cells.push((
                &self.s2_insecure,
                &[
                    "insecure",
                    "allowInsecure",
                    "allow_insecure",
                    "allowinsecure",
                ][..],
            ));
        }
        if mux_page {
            shared_cells.push((&self.s3_brutal, &["brutal_enable"][..]));
        }
        for (cell, keys) in shared_cells {
            if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                *cell.borrow_mut() = true;
            }
        }

        if (full_panels || optional_tls) && !*self.s2_enable.borrow() {
            let on = if protocol == "vmess" {
                vmess_tls_enabled(box_text)
            } else {
                let security = guided_prefill_value(box_text, &["security"]);
                matches!(
                    security.to_ascii_lowercase().as_str(),
                    "tls" | "xtls" | "reality"
                )
            };
            if on {
                *self.s2_enable.borrow_mut() = true;
            }
        }
        if protocol == "shadowsocks"
            && !*self.ss_uot.borrow()
            && guided_prefill_bool(box_text, &["udp_over_tcp", "uot"])
        {
            *self.ss_uot.borrow_mut() = true;
        }

        if protocol == "socks"
            && !*self.socks_uot.borrow()
            && guided_prefill_bool(box_text, &["udp_over_tcp", "uot"])
        {
            *self.socks_uot.borrow_mut() = true;
        }
        if protocol == "naive" {
            for (cell, keys) in [
                (&self.naive_uot, &["udp_over_tcp", "uot"][..]),
                (&self.naive_quic, &["quic"][..]),
            ] {
                if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                    *cell.borrow_mut() = true;
                }
            }
        }

        if matches!(protocol, "hysteria" | "hysteria2")
            && !*self.hy_mtu.borrow()
            && guided_prefill_bool(box_text, &["disable_mtu_discovery"])
        {
            *self.hy_mtu.borrow_mut() = true;
        }
        if protocol == "tuic" {
            for (cell, keys) in [
                (&self.tuic_uot, &["udp_over_stream"][..]),
                (&self.tuic_zero_rtt, &["zero_rtt_handshake", "zero_rtt"][..]),
            ] {
                if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                    *cell.borrow_mut() = true;
                }
            }
        }

        if protocol == "wireguard"
            && !*self.wg_sys_ifc.borrow()
            && guided_prefill_bool(box_text, &["system", "sys_ifc"])
        {
            *self.wg_sys_ifc.borrow_mut() = true;
        }
        if protocol == "amneziawg" {
            for (cell, keys) in [
                (&self.awg_sys_ifc, &["system", "sys_ifc"][..]),
                (&self.awg_enable, &["enable_amnezia"][..]),
                (
                    &self.awg_trailers,
                    &["randomtrailers", "random_trailers"][..],
                ),
                (
                    &self.awg_cookies,
                    &["disablecookies", "disable_cookies"][..],
                ),
            ] {
                if !*cell.borrow() && guided_prefill_bool(box_text, keys) {
                    *cell.borrow_mut() = true;
                }
            }
        }
        if mux_page && self.s3_mode.borrow().is_empty() {
            let mode = guided_prefill_value(box_text, &["mux"]);
            if !mode.is_empty() {
                *self.s3_mode.borrow_mut() = mode;
            }
        }
    }

    pub(crate) fn as_patch_vs(&self, cx: &App, baseline: &ProxyConfig) -> ConfigPatch {
        fn changed(current: String, baseline: &str) -> Option<String> {
            if current == baseline {
                None
            } else {
                Some(current)
            }
        }
        let current = self.as_new(cx);
        ConfigPatch {
            name: changed(current.name, &baseline.name),
            protocol: changed(current.protocol, &baseline.protocol),
            params: changed(current.params, &baseline.params),
            group: changed(current.group, &baseline.group),
            bind_ip: changed(current.bind_ip, &baseline.bind_ip),
            port: changed(current.port, &baseline.port.to_string()),
        }
    }
}
