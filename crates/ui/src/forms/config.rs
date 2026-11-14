//! The config add/edit form: protocol pickers, field validation, params folding.
//! Belongs: form fields and their pure validation/fold helpers.
//! Not here: store commits or dialog chrome.
use crate::*;

/// K2: the protocol picker renders its value inside the trigger like any other
/// select. `pickers::protocol::ProtocolChoice` prefixes the trigger with
/// `Protocol: ` (its own display_title), which is the label-repeats-value quirk
/// the ticket removes — and that picker is not ours to edit, so the dialog owns
/// this delegate. Item ids stay `protocol-option-{protocol}` so the UIA
/// fixtures and option clicks keep their stable anchors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogProtocolChoice {
    pub id: String,
    pub title: String,
    pub value: String,
}

impl SearchableListItem for DialogProtocolChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone().into()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DialogProtocolChoiceList(pub Vec<DialogProtocolChoice>);

impl SearchableListDelegate for DialogProtocolChoiceList {
    type Item = DialogProtocolChoice;

    fn items_count(&self, _section: usize) -> usize {
        self.0.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&DialogProtocolChoice> {
        self.0.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        DialogProtocolChoice: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.0
            .iter()
            .position(|choice| choice.value() == value)
            .map(IndexPath::new)
    }

    fn render_item(
        &self,
        _ix: IndexPath,
        item: &DialogProtocolChoice,
        _checked: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<AnyElement> {
        Some(
            div()
                .id(item.id.clone())
                .test_support()
                .child(item.title.clone())
                .into_any_element(),
        )
    }
}

pub fn dialog_protocol_choices() -> Vec<DialogProtocolChoice> {
    protocol_choices()
        .into_iter()
        .map(|choice| DialogProtocolChoice {
            id: format!("protocol-option-{}", choice.value),
            title: choice.value.clone(),
            value: choice.value,
        })
        .collect()
}

pub fn password_row(
    label: impl Into<String>,
    id: &'static str,
    secret: &Entity<InputState>,
    cx: &App,
) -> Field {
    let label = label.into();
    let presence = secret_state_caption(secret.read(cx).value().as_ref()).to_string();
    field().label(label.clone()).child(
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(labelled_masked_input(label, secret).id(id))
            .child(
                div()
                    .id(format!("{id}-state"))
                    .test_support()
                    .text_sm()
                    .child(presence),
            ),
    )
}

pub fn panel_toggle_button(
    id: &'static str,
    label: impl Into<SharedString>,
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

/// F5: a guided field's error belongs beside that field, not at the form foot.
pub fn local_field_error(
    id: &'static str,
    form_error: Option<&(String, String)>,
    field: &str,
    cx: &App,
) -> Option<AnyElement> {
    let (name, message) = form_error?;
    (name == field).then(|| {
        div()
            .id(id)
            .test_support()
            .text_sm()
            .text_color(danger_text_color(cx.theme()))
            .child(message.clone())
            .into_any_element()
    })
}

#[derive(Clone)]
pub struct ConfigForm {
    pub name: Entity<InputState>,

    pub protocol_select: Entity<SelectState<DialogProtocolChoiceList>>,

    pub last_protocol: Rc<RefCell<String>>,

    pub protocol_locked: bool,

    pub locked_protocol: String,

    pub vless_uuid: Entity<InputState>,
    pub vless_flow: Entity<SelectState<PanelChoiceList>>,
    pub vless_packet_encoding: Entity<SelectState<PanelChoiceList>>,

    pub s1_network: Entity<SelectState<PanelChoiceList>>,
    pub s1_path: Entity<InputState>,
    pub s1_host: Entity<InputState>,
    pub s1_method: Entity<InputState>,
    pub s1_service_name: Entity<InputState>,
    pub s1_open: Rc<RefCell<bool>>,

    pub s2_enable: Rc<RefCell<bool>>,
    pub s2_sni: Entity<InputState>,
    pub s2_alpn: Entity<InputState>,
    pub s2_fp: Entity<SelectState<PanelChoiceList>>,
    pub s2_insecure: Rc<RefCell<bool>>,
    pub s2_pbk: Entity<InputState>,
    pub s2_sid: Entity<InputState>,
    pub s2_open: Rc<RefCell<bool>>,

    pub s3_mode: Rc<RefCell<String>>,
    pub s3_brutal: Rc<RefCell<bool>>,
    pub s3_up: Entity<InputState>,
    pub s3_down: Entity<InputState>,
    pub s3_open: Rc<RefCell<bool>>,

    pub s4_reuse: Rc<RefCell<bool>>,
    pub s4_fastopen: Rc<RefCell<bool>>,
    pub s4_fragment: Rc<RefCell<bool>>,
    pub s4_multipath: Rc<RefCell<bool>>,
    pub s4_timeout: Entity<InputState>,
    pub s4_bind: Entity<InputState>,
    pub s4_inet4: Entity<InputState>,
    pub s4_inet6: Entity<InputState>,

    pub vmess_uuid: Entity<InputState>,
    pub vmess_aid: Entity<InputState>,
    pub vmess_security: Entity<SelectState<PanelChoiceList>>,
    pub vmess_packet_encoding: Entity<SelectState<PanelChoiceList>>,

    pub trojan_password: Entity<InputState>,

    pub ss_method: Entity<SelectState<PanelChoiceList>>,
    pub ss_password: Entity<InputState>,
    pub ss_plugin: Entity<SelectState<PanelChoiceList>>,
    pub ss_plugin_opts: Entity<InputState>,
    pub ss_uot: Rc<RefCell<bool>>,

    pub wg_private: Entity<InputState>,
    pub wg_public: Entity<InputState>,
    pub wg_preshared: Entity<InputState>,
    pub wg_reserved: Entity<InputState>,
    pub wg_keepalive: Entity<InputState>,
    pub wg_local_addr: Entity<InputState>,
    pub wg_dns: Entity<InputState>,
    pub wg_mtu: Entity<InputState>,
    pub wg_workers: Entity<InputState>,
    pub wg_sys_ifc: Rc<RefCell<bool>>,

    pub awg_private: Entity<InputState>,
    pub awg_public: Entity<InputState>,
    pub awg_preshared: Entity<InputState>,
    pub awg_reserved: Entity<InputState>,
    pub awg_keepalive: Entity<InputState>,
    pub awg_local_addr: Entity<InputState>,
    pub awg_dns: Entity<InputState>,
    pub awg_mtu: Entity<InputState>,
    pub awg_workers: Entity<InputState>,
    pub awg_sys_ifc: Rc<RefCell<bool>>,
    pub awg_version: Entity<SelectState<PanelChoiceList>>,
    pub awg_enable: Rc<RefCell<bool>>,
    pub awg_jc: Entity<InputState>,
    pub awg_jmin: Entity<InputState>,
    pub awg_jmax: Entity<InputState>,
    pub awg_s1: Entity<InputState>,
    pub awg_s2: Entity<InputState>,
    pub awg_s3: Entity<InputState>,
    pub awg_s4: Entity<InputState>,
    pub awg_h1: Entity<InputState>,
    pub awg_h2: Entity<InputState>,
    pub awg_h3: Entity<InputState>,
    pub awg_h4: Entity<InputState>,
    pub awg_i1: Entity<InputState>,
    pub awg_i2: Entity<InputState>,
    pub awg_i3: Entity<InputState>,
    pub awg_i4: Entity<InputState>,
    pub awg_i5: Entity<InputState>,
    pub awg_hpk: Entity<InputState>,
    pub awg_pad: Entity<InputState>,
    pub awg_rekey_after: Entity<InputState>,
    pub awg_rekey_timeout: Entity<InputState>,
    pub awg_reject_after: Entity<InputState>,
    pub awg_keepalive_timeout: Entity<InputState>,
    pub awg_max_handshake: Entity<InputState>,
    pub awg_trailers: Rc<RefCell<bool>>,
    pub awg_cookies: Rc<RefCell<bool>>,

    pub hy_server_ports: Entity<InputState>,
    pub hy_hop_interval: Entity<InputState>,
    pub hy_up: Entity<InputState>,
    pub hy_down: Entity<InputState>,
    pub hy_obfs: Entity<InputState>,
    pub hy_auth_type: Entity<SelectState<PanelChoiceList>>,
    pub hy_auth: Entity<InputState>,
    pub hy_recv_conn: Entity<InputState>,
    pub hy_recv: Entity<InputState>,
    pub hy_mtu: Rc<RefCell<bool>>,
    pub hy_password: Entity<InputState>,

    pub tuic_uuid: Entity<InputState>,
    pub tuic_password: Entity<InputState>,
    pub tuic_cc: Entity<SelectState<PanelChoiceList>>,
    pub tuic_relay: Entity<SelectState<PanelChoiceList>>,
    pub tuic_uot: Rc<RefCell<bool>>,
    pub tuic_zero_rtt: Rc<RefCell<bool>>,
    pub tuic_heartbeat: Entity<InputState>,

    pub socks_version: Entity<SelectState<PanelChoiceList>>,
    pub socks_username: Entity<InputState>,
    pub socks_password: Entity<InputState>,
    pub socks_network: Entity<SelectState<PanelChoiceList>>,
    pub socks_uot: Rc<RefCell<bool>>,

    pub http_username: Entity<InputState>,
    pub http_password: Entity<InputState>,
    pub http_path: Entity<InputState>,
    pub http_headers: Entity<InputState>,

    pub anytls_password: Entity<InputState>,
    pub anytls_interval: Entity<InputState>,
    pub anytls_timeout: Entity<InputState>,
    pub anytls_min: Entity<InputState>,

    pub naive_username: Entity<InputState>,
    pub naive_password: Entity<InputState>,
    pub naive_uot: Rc<RefCell<bool>>,
    pub naive_quic: Rc<RefCell<bool>>,
    pub naive_cc: Entity<SelectState<PanelChoiceList>>,

    pub ssh_user: Entity<InputState>,
    pub ssh_password: Entity<InputState>,
    pub ssh_private_key: Entity<TextareaState>,
    pub ssh_key_path: Entity<InputState>,
    pub ssh_key_pass: Entity<InputState>,
    pub ssh_host_key: Entity<InputState>,
    pub ssh_host_key_algs: Entity<InputState>,
    pub ssh_client_version: Entity<InputState>,

    pub snell_psk: Entity<InputState>,
    pub snell_version: Entity<SelectState<PanelChoiceList>>,
    pub snell_obfs_mode: Entity<InputState>,
    pub snell_obfs_host: Entity<InputState>,
    pub snell_userkey: Entity<InputState>,
    pub snell_reuse: Rc<RefCell<bool>>,
    pub snell_network: Entity<InputState>,
    pub snell_mode: Entity<InputState>,

    pub shadowtls_version: Entity<SelectState<PanelChoiceList>>,
    pub shadowtls_password: Entity<InputState>,

    pub xray_uuid: Entity<InputState>,
    pub xray_enc: Entity<InputState>,
    pub xray_flow: Entity<SelectState<PanelChoiceList>>,
    pub xray_sni: Entity<InputState>,
    pub xray_fp: Entity<SelectState<PanelChoiceList>>,
    pub xray_alpn: Entity<InputState>,
    pub xray_pbk: Entity<InputState>,
    pub xray_sid: Entity<InputState>,
    pub xray_spiderx: Entity<InputState>,
    pub xray_network: Entity<SelectState<PanelChoiceList>>,
    pub xray_mode: Entity<InputState>,
    pub xray_mux: Entity<SelectState<PanelChoiceList>>,

    pub chain_hops: Rc<RefCell<Vec<serde_json::Value>>>,

    pub chain_pick: Entity<SelectState<PanelChoiceList>>,

    pub chain_library: Rc<RefCell<Vec<ChainLibraryEntry>>>,

    pub extra_exe: Entity<InputState>,
    pub extra_args: Entity<InputState>,
    pub extra_config: Entity<TextareaState>,

    pub params: Entity<TextareaState>,
    pub group: Entity<InputState>,
    pub bind: Entity<InputState>,
    pub port: Entity<InputState>,

    pub test_url: Entity<InputState>,

    pub guided_server: Entity<InputState>,
    pub guided_port: Entity<InputState>,
    pub guided_identity: Entity<InputState>,

    pub advanced: Rc<RefCell<bool>>,
}

impl ConfigForm {
    pub fn field(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: impl Into<String>,
        default: Option<&str>,
    ) -> Entity<InputState> {
        let placeholder = placeholder.into();
        cx.new(|cx| {
            let state = InputState::new(window, cx).placeholder(placeholder);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    pub fn secret_field(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: impl Into<String>,
        default: Option<&str>,
    ) -> Entity<InputState> {
        let placeholder = placeholder.into();
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

    pub fn panel_select(
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

    pub fn area(
        window: &mut Window,
        cx: &mut Context<AppView>,
        placeholder: impl Into<String>,
        default: Option<&str>,
    ) -> Entity<TextareaState> {
        let placeholder = placeholder.into();
        cx.new(|cx| {
            let state = TextareaState::new(window, cx).placeholder(placeholder);
            match default {
                Some(value) => state.default_value(value.to_string()),
                None => state,
            }
        })
    }

    pub fn protocol_text(&self, cx: &App) -> String {
        if self.protocol_locked {
            return self.locked_protocol.clone();
        }
        self.protocol_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_else(|| self.last_protocol.borrow().clone())
    }

    pub fn sync_protocol_template(&self, window: &mut Window, cx: &mut App) {
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

    pub fn blank(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        let template = protocol_params_template(DEFAULT_PROTOCOL);
        let selected = dialog_protocol_choices()
            .iter()
            .position(|choice| choice.value == DEFAULT_PROTOCOL)
            .map(IndexPath::new);
        Self {
            name: Self::field(window, cx, t!("config-name"), None),
            protocol_select: cx.new(|cx| {
                SelectState::new(
                    DialogProtocolChoiceList(dialog_protocol_choices()),
                    selected,
                    window,
                    cx,
                )
            }),
            last_protocol: Rc::new(RefCell::new(DEFAULT_PROTOCOL.to_string())),
            protocol_locked: false,
            locked_protocol: DEFAULT_PROTOCOL.to_string(),
            params: Self::area(window, cx, t!("config-params-placeholder"), Some(&template)),
            group: Self::field(window, cx, t!("config-group"), Some(DEFAULT_GROUP)),
            bind: Self::field(window, cx, t!("config-bind"), Some(DEFAULT_BIND)),
            port: Self::field(window, cx, "auto", None),
            test_url: Self::field(window, cx, t!("config-test-url"), None),
            guided_server: Self::field(window, cx, t!("config-server-placeholder"), None),
            guided_port: Self::field(window, cx, t!("config-port-placeholder"), None),
            guided_identity: Self::secret_field(window, cx, t!("config-identity-secret"), None),

            vless_uuid: Self::secret_field(window, cx, t!("config-uuid"), None),
            vless_flow: Self::panel_select(window, cx, flow_choices(), ""),
            vless_packet_encoding: Self::panel_select(window, cx, packet_encoding_choices(), ""),
            s1_network: Self::panel_select(window, cx, network_choices(), "tcp"),
            s1_path: Self::field(window, cx, t!("config-path"), None),
            s1_host: Self::field(window, cx, t!("config-host"), None),
            s1_method: Self::field(window, cx, t!("config-method"), None),
            s1_service_name: Self::field(window, cx, t!("config-service-name"), None),
            s1_open: Rc::new(RefCell::new(false)),
            s2_enable: Rc::new(RefCell::new(false)),
            s2_sni: Self::field(window, cx, t!("config-sni"), None),
            s2_alpn: Self::field(window, cx, t!("config-alpn"), None),
            s2_fp: Self::panel_select(window, cx, fp_choices(), ""),
            s2_insecure: Rc::new(RefCell::new(false)),
            s2_pbk: Self::field(window, cx, t!("config-reality-public-key"), None),
            s2_sid: Self::field(window, cx, t!("config-reality-short-id"), None),
            s2_open: Rc::new(RefCell::new(false)),
            s3_mode: Rc::new(RefCell::new(String::new())),
            s3_brutal: Rc::new(RefCell::new(false)),
            s3_up: Self::field(window, cx, t!("config-brutal-up"), None),
            s3_down: Self::field(window, cx, t!("config-brutal-down"), None),
            s3_open: Rc::new(RefCell::new(false)),
            s4_reuse: Rc::new(RefCell::new(false)),
            s4_fastopen: Rc::new(RefCell::new(false)),
            s4_fragment: Rc::new(RefCell::new(false)),
            s4_multipath: Rc::new(RefCell::new(false)),
            s4_timeout: Self::field(window, cx, t!("config-connect-timeout-placeholder"), None),
            s4_bind: Self::field(window, cx, t!("config-bind-interface"), None),
            s4_inet4: Self::field(window, cx, t!("config-ipv4-bind"), None),
            s4_inet6: Self::field(window, cx, t!("config-ipv6-bind"), None),

            vmess_uuid: Self::secret_field(window, cx, t!("config-uuid"), None),
            vmess_aid: Self::field(window, cx, t!("config-alter-id-placeholder"), None),
            vmess_security: Self::panel_select(window, cx, vmess_security_choices(), "auto"),
            vmess_packet_encoding: Self::panel_select(window, cx, packet_encoding_choices(), ""),
            trojan_password: Self::secret_field(window, cx, t!("config-password"), None),
            ss_method: Self::panel_select(window, cx, ss_method_choices(), ""),
            ss_password: Self::secret_field(window, cx, t!("config-password"), None),
            ss_plugin: Self::panel_select(window, cx, ss_plugin_choices(), ""),
            ss_plugin_opts: Self::field(window, cx, t!("config-plugin-options"), None),
            ss_uot: Rc::new(RefCell::new(false)),

            wg_private: Self::secret_field(window, cx, t!("config-private-key"), None),
            wg_public: Self::field(window, cx, t!("config-peer-public-key"), None),
            wg_preshared: Self::secret_field(window, cx, t!("config-preshared-key"), None),
            wg_reserved: Self::field(window, cx, t!("config-reserved"), None),
            wg_keepalive: Self::field(window, cx, t!("config-keepalive"), None),
            wg_local_addr: Self::field(window, cx, t!("config-local-address"), None),
            wg_dns: Self::field(window, cx, t!("config-dns"), None),
            wg_mtu: Self::field(window, cx, t!("config-mtu"), None),
            wg_workers: Self::field(window, cx, t!("config-workers"), None),
            wg_sys_ifc: Rc::new(RefCell::new(false)),
            awg_private: Self::secret_field(window, cx, t!("config-private-key"), None),
            awg_public: Self::field(window, cx, t!("config-peer-public-key"), None),
            awg_preshared: Self::secret_field(window, cx, t!("config-preshared-key"), None),
            awg_reserved: Self::field(window, cx, t!("config-reserved"), None),
            awg_keepalive: Self::field(window, cx, t!("config-keepalive"), None),
            awg_local_addr: Self::field(window, cx, t!("config-local-address"), None),
            awg_dns: Self::field(window, cx, t!("config-dns"), None),
            awg_mtu: Self::field(window, cx, t!("config-mtu"), None),
            awg_workers: Self::field(window, cx, t!("config-workers"), None),
            awg_sys_ifc: Rc::new(RefCell::new(false)),
            awg_version: Self::panel_select(window, cx, awg_version_choices(), ""),
            awg_enable: Rc::new(RefCell::new(false)),
            awg_jc: Self::field(window, cx, t!("config-jc"), None),
            awg_jmin: Self::field(window, cx, t!("config-jmin"), None),
            awg_jmax: Self::field(window, cx, t!("config-jmax"), None),
            awg_s1: Self::field(window, cx, t!("config-s1"), None),
            awg_s2: Self::field(window, cx, t!("config-s2"), None),
            awg_s3: Self::field(window, cx, t!("config-s3"), None),
            awg_s4: Self::field(window, cx, t!("config-s4"), None),
            awg_h1: Self::field(window, cx, t!("config-h1"), None),
            awg_h2: Self::field(window, cx, t!("config-h2"), None),
            awg_h3: Self::field(window, cx, t!("config-h3"), None),
            awg_h4: Self::field(window, cx, t!("config-h4"), None),
            awg_i1: Self::field(window, cx, t!("config-i1"), None),
            awg_i2: Self::field(window, cx, t!("config-i2"), None),
            awg_i3: Self::field(window, cx, t!("config-i3"), None),
            awg_i4: Self::field(window, cx, t!("config-i4"), None),
            awg_i5: Self::field(window, cx, t!("config-i5"), None),
            awg_hpk: Self::field(window, cx, t!("config-header-protection-key"), None),
            awg_pad: Self::field(window, cx, t!("config-content-padding"), None),
            awg_rekey_after: Self::field(window, cx, t!("config-rekey-after"), None),
            awg_rekey_timeout: Self::field(window, cx, t!("config-rekey-timeout"), None),
            awg_reject_after: Self::field(window, cx, t!("config-reject-after"), None),
            awg_keepalive_timeout: Self::field(window, cx, t!("config-keepalive-timeout"), None),
            awg_max_handshake: Self::field(window, cx, t!("config-max-handshake"), None),
            awg_trailers: Rc::new(RefCell::new(false)),
            awg_cookies: Rc::new(RefCell::new(false)),

            hy_server_ports: Self::field(window, cx, t!("config-server-ports-placeholder"), None),
            hy_hop_interval: Self::field(window, cx, t!("config-hop-interval-placeholder"), None),
            hy_up: Self::field(window, cx, t!("config-up"), None),
            hy_down: Self::field(window, cx, t!("config-down"), None),
            hy_obfs: Self::field(window, cx, t!("config-obfs"), None),
            hy_auth_type: Self::panel_select(window, cx, hy_auth_type_choices(), ""),
            hy_auth: Self::secret_field(window, cx, t!("config-auth-secret"), None),
            hy_recv_conn: Self::field(window, cx, t!("config-recv-window-conn"), None),
            hy_recv: Self::field(window, cx, t!("config-recv-window"), None),
            hy_mtu: Rc::new(RefCell::new(false)),
            hy_password: Self::secret_field(window, cx, t!("config-password"), None),
            tuic_uuid: Self::secret_field(window, cx, t!("config-uuid"), None),
            tuic_password: Self::secret_field(window, cx, t!("config-password"), None),
            tuic_cc: Self::panel_select(window, cx, tuic_cc_choices(), ""),
            tuic_relay: Self::panel_select(window, cx, tuic_relay_choices(), ""),
            tuic_uot: Rc::new(RefCell::new(false)),
            tuic_zero_rtt: Rc::new(RefCell::new(false)),
            tuic_heartbeat: Self::field(window, cx, t!("config-heartbeat-placeholder"), None),

            socks_version: Self::panel_select(window, cx, socks_version_choices(), ""),
            socks_username: Self::field(window, cx, t!("config-username"), None),
            socks_password: Self::secret_field(window, cx, t!("config-password"), None),
            socks_network: Self::panel_select(window, cx, socks_network_choices(), "tcp"),
            socks_uot: Rc::new(RefCell::new(false)),
            http_username: Self::field(window, cx, t!("config-username"), None),
            http_password: Self::secret_field(window, cx, t!("config-password"), None),
            http_path: Self::field(window, cx, t!("config-path"), None),
            http_headers: Self::field(window, cx, t!("config-headers"), None),
            anytls_password: Self::secret_field(window, cx, t!("config-password"), None),
            anytls_interval: Self::field(window, cx, t!("config-idle-check-placeholder"), None),
            anytls_timeout: Self::field(window, cx, t!("config-idle-timeout-placeholder"), None),
            anytls_min: Self::field(window, cx, t!("config-min-idle"), None),
            naive_username: Self::field(window, cx, t!("config-username"), None),
            naive_password: Self::secret_field(window, cx, t!("config-password"), None),
            naive_uot: Rc::new(RefCell::new(false)),
            naive_quic: Rc::new(RefCell::new(false)),
            naive_cc: Self::panel_select(window, cx, naive_cc_choices(), ""),
            ssh_user: Self::field(window, cx, t!("config-user"), None),
            ssh_password: Self::secret_field(window, cx, t!("config-password"), None),
            ssh_private_key: Self::area(window, cx, t!("config-private-key-paste"), None),
            ssh_key_path: Self::field(window, cx, t!("config-private-key-path"), None),
            ssh_key_pass: Self::secret_field(window, cx, t!("config-private-key-passphrase"), None),
            ssh_host_key: Self::field(window, cx, t!("config-host-key"), None),
            ssh_host_key_algs: Self::field(window, cx, t!("config-host-key-algs"), None),
            ssh_client_version: Self::field(window, cx, t!("config-client-version"), None),
            snell_psk: Self::secret_field(window, cx, t!("config-psk"), None),
            snell_version: Self::panel_select(window, cx, snell_version_choices(), ""),
            snell_obfs_mode: Self::field(window, cx, t!("config-obfs-mode"), None),
            snell_obfs_host: Self::field(window, cx, t!("config-obfs-host"), None),
            snell_userkey: Self::field(window, cx, t!("config-user-key"), None),
            snell_reuse: Rc::new(RefCell::new(false)),
            snell_network: Self::field(window, cx, t!("config-network"), None),
            snell_mode: Self::field(window, cx, t!("config-mode"), None),
            shadowtls_version: Self::panel_select(window, cx, shadowtls_version_choices(), ""),
            shadowtls_password: Self::secret_field(window, cx, t!("config-password"), None),
            xray_uuid: Self::secret_field(window, cx, t!("config-uuid"), None),
            xray_enc: Self::field(window, cx, t!("config-encryption-placeholder"), None),
            xray_flow: Self::panel_select(window, cx, xray_flow_choices(), ""),
            xray_sni: Self::field(window, cx, t!("config-sni"), None),
            xray_fp: Self::panel_select(window, cx, fp_choices(), ""),
            xray_alpn: Self::field(window, cx, t!("config-alpn"), None),
            xray_pbk: Self::field(window, cx, t!("config-reality-pbk"), None),
            xray_sid: Self::field(window, cx, t!("config-reality-sid"), None),
            xray_spiderx: Self::field(window, cx, t!("config-reality-spiderx"), None),
            xray_network: Self::panel_select(window, cx, xray_network_choices(), ""),
            xray_mode: Self::field(window, cx, t!("config-mode"), None),
            xray_mux: Self::panel_select(window, cx, xray_mux_choices(), ""),
            chain_hops: Rc::new(RefCell::new(Vec::new())),
            chain_pick: Self::panel_select(window, cx, chain_pick_choices(&[]), ""),
            chain_library: Rc::new(RefCell::new(Vec::new())),
            extra_exe: Self::field(window, cx, t!("config-executable-placeholder"), None),
            extra_args: Self::field(window, cx, t!("config-args"), None),
            extra_config: Self::area(window, cx, t!("config-config-json-placeholder"), None),
            advanced: Rc::new(RefCell::new(false)),
        }
    }

    pub fn prefilled(
        window: &mut Window,
        cx: &mut Context<AppView>,
        config: &ProxyConfig,
        test_url: Option<&str>,
    ) -> Self {
        let read_keys: &[&str] = guided_identity_fields(&config.protocol)
            .map(|guided| guided.read_keys)
            .unwrap_or(&[]);
        let selected = dialog_protocol_choices()
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
            name: Self::field(window, cx, t!("config-name"), Some(config.name.as_str())),
            protocol_select: cx.new(|cx| {
                SelectState::new(
                    DialogProtocolChoiceList(dialog_protocol_choices()),
                    selected,
                    window,
                    cx,
                )
            }),
            last_protocol: Rc::new(RefCell::new(config.protocol.clone())),
            protocol_locked: true,
            locked_protocol: config.protocol.clone(),
            params: Self::area(
                window,
                cx,
                t!("config-params-placeholder"),
                Some(config.params.as_str()),
            ),
            group: Self::field(window, cx, t!("config-group"), Some(config.group.as_str())),
            bind: Self::field(window, cx, t!("config-bind"), Some(config.bind_ip.as_str())),
            port: Self::field(window, cx, "auto", Some(&config.port.to_string())),
            test_url: Self::field(window, cx, t!("config-test-url"), test_url),
            guided_server: Self::field(
                window,
                cx,
                t!("config-server-placeholder"),
                Some(&guided_prefill_value(
                    &config.params,
                    &["server", "address"],
                )),
            ),
            guided_port: Self::field(
                window,
                cx,
                t!("config-port-placeholder"),
                Some(&guided_prefill_value(
                    &config.params,
                    &["server_port", "port"],
                )),
            ),
            guided_identity: Self::secret_field(
                window,
                cx,
                t!("config-identity-secret"),
                Some(&guided_prefill_value(&config.params, read_keys)),
            ),
            vless_uuid: Self::secret_field(
                window,
                cx,
                t!("config-uuid"),
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
                t!("config-path"),
                Some(&guided_prefill_value(params, &["path"])),
            ),
            s1_host: Self::field(
                window,
                cx,
                t!("config-host"),
                Some(&guided_prefill_value(params, &["host"])),
            ),
            s1_method: Self::field(
                window,
                cx,
                t!("config-method"),
                Some(&guided_prefill_value(params, &["method"])),
            ),
            s1_service_name: Self::field(
                window,
                cx,
                t!("config-service-name"),
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
                t!("config-sni"),
                Some(&guided_prefill_value(params, &["sni", "peer"])),
            ),
            s2_alpn: Self::field(
                window,
                cx,
                t!("config-alpn"),
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
                t!("config-reality-public-key"),
                Some(&guided_prefill_value(params, &["pbk"])),
            ),
            s2_sid: Self::field(
                window,
                cx,
                t!("config-reality-short-id"),
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
                t!("config-brutal-up"),
                Some(&guided_prefill_value(
                    params,
                    &["brutal_up_mbps", "brutal_up"],
                )),
            ),
            s3_down: Self::field(
                window,
                cx,
                t!("config-brutal-down"),
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
                t!("config-connect-timeout-placeholder"),
                Some(&guided_prefill_value(params, &["connect_timeout"])),
            ),
            s4_bind: Self::field(
                window,
                cx,
                t!("config-bind-interface"),
                Some(&guided_prefill_value(params, &["bind_interface"])),
            ),
            s4_inet4: Self::field(
                window,
                cx,
                t!("config-ipv4-bind"),
                Some(&guided_prefill_value(params, &["inet4_bind_address"])),
            ),
            s4_inet6: Self::field(
                window,
                cx,
                t!("config-ipv6-bind"),
                Some(&guided_prefill_value(params, &["inet6_bind_address"])),
            ),

            vmess_uuid: Self::secret_field(
                window,
                cx,
                t!("config-uuid"),
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            vmess_aid: Self::field(
                window,
                cx,
                t!("config-alter-id-placeholder"),
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
            trojan_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
                Some(&guided_prefill_value(params, &["password"])),
            ),
            ss_method: Self::panel_select(
                window,
                cx,
                ss_method_choices(),
                &guided_prefill_value(params, &["method"]),
            ),
            ss_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
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
                t!("config-plugin-options"),
                Some(&guided_prefill_value(
                    params,
                    &["plugin_opts", "plugin-opts", "pluginOpts"],
                )),
            ),
            ss_uot: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["udp_over_tcp", "uot"],
            ))),

            wg_private: Self::secret_field(
                window,
                cx,
                t!("config-private-key"),
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            wg_public: Self::field(
                window,
                cx,
                t!("config-peer-public-key"),
                Some(&guided_prefill_value(params, &["public_key", "publickey"])),
            ),
            wg_preshared: Self::secret_field(
                window,
                cx,
                t!("config-preshared-key"),
                Some(&guided_prefill_value(
                    params,
                    &["pre_shared_key", "presharedkey", "preshared_key"],
                )),
            ),
            wg_reserved: Self::field(
                window,
                cx,
                t!("config-reserved"),
                Some(&guided_prefill_value(params, &["reserved"])),
            ),
            wg_keepalive: Self::field(
                window,
                cx,
                t!("config-keepalive"),
                Some(&guided_prefill_value(
                    params,
                    &["persistent_keepalive", "persistent_keepalive_interval"],
                )),
            ),
            wg_local_addr: Self::field(
                window,
                cx,
                t!("config-local-address"),
                Some(&guided_prefill_value(
                    params,
                    &["address", "local_address", "local_addr"],
                )),
            ),
            wg_dns: Self::field(
                window,
                cx,
                t!("config-dns"),
                Some(&guided_prefill_value(params, &["dns"])),
            ),
            wg_mtu: Self::field(
                window,
                cx,
                t!("config-mtu"),
                Some(&guided_prefill_value(params, &["mtu"])),
            ),
            wg_workers: Self::field(
                window,
                cx,
                t!("config-workers"),
                Some(&guided_prefill_value(params, &["workers"])),
            ),
            wg_sys_ifc: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["system", "sys_ifc"],
            ))),
            awg_private: Self::secret_field(
                window,
                cx,
                t!("config-private-key"),
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            awg_public: Self::field(
                window,
                cx,
                t!("config-peer-public-key"),
                Some(&guided_prefill_value(params, &["public_key", "publickey"])),
            ),
            awg_preshared: Self::secret_field(
                window,
                cx,
                t!("config-preshared-key"),
                Some(&guided_prefill_value(
                    params,
                    &["pre_shared_key", "presharedkey", "preshared_key"],
                )),
            ),
            awg_reserved: Self::field(
                window,
                cx,
                t!("config-reserved"),
                Some(&guided_prefill_value(params, &["reserved"])),
            ),
            awg_keepalive: Self::field(
                window,
                cx,
                t!("config-keepalive"),
                Some(&guided_prefill_value(
                    params,
                    &["persistent_keepalive", "persistent_keepalive_interval"],
                )),
            ),
            awg_local_addr: Self::field(
                window,
                cx,
                t!("config-local-address"),
                Some(&guided_prefill_value(
                    params,
                    &["address", "local_address", "local_addr"],
                )),
            ),
            awg_dns: Self::field(
                window,
                cx,
                t!("config-dns"),
                Some(&guided_prefill_value(params, &["dns"])),
            ),
            awg_mtu: Self::field(
                window,
                cx,
                t!("config-mtu"),
                Some(&guided_prefill_value(params, &["mtu"])),
            ),
            awg_workers: Self::field(
                window,
                cx,
                t!("config-workers"),
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
                t!("config-jc"),
                Some(&guided_prefill_value(params, &["jc"])),
            ),
            awg_jmin: Self::field(
                window,
                cx,
                t!("config-jmin"),
                Some(&guided_prefill_value(params, &["jmin"])),
            ),
            awg_jmax: Self::field(
                window,
                cx,
                t!("config-jmax"),
                Some(&guided_prefill_value(params, &["jmax"])),
            ),
            awg_s1: Self::field(
                window,
                cx,
                t!("config-s1"),
                Some(&guided_prefill_value(params, &["s1"])),
            ),
            awg_s2: Self::field(
                window,
                cx,
                t!("config-s2"),
                Some(&guided_prefill_value(params, &["s2"])),
            ),
            awg_s3: Self::field(
                window,
                cx,
                t!("config-s3"),
                Some(&guided_prefill_value(params, &["s3"])),
            ),
            awg_s4: Self::field(
                window,
                cx,
                t!("config-s4"),
                Some(&guided_prefill_value(params, &["s4"])),
            ),
            awg_h1: Self::field(
                window,
                cx,
                t!("config-h1"),
                Some(&guided_prefill_value(params, &["h1"])),
            ),
            awg_h2: Self::field(
                window,
                cx,
                t!("config-h2"),
                Some(&guided_prefill_value(params, &["h2"])),
            ),
            awg_h3: Self::field(
                window,
                cx,
                t!("config-h3"),
                Some(&guided_prefill_value(params, &["h3"])),
            ),
            awg_h4: Self::field(
                window,
                cx,
                t!("config-h4"),
                Some(&guided_prefill_value(params, &["h4"])),
            ),
            awg_i1: Self::field(
                window,
                cx,
                t!("config-i1"),
                Some(&guided_prefill_value(params, &["i1"])),
            ),
            awg_i2: Self::field(
                window,
                cx,
                t!("config-i2"),
                Some(&guided_prefill_value(params, &["i2"])),
            ),
            awg_i3: Self::field(
                window,
                cx,
                t!("config-i3"),
                Some(&guided_prefill_value(params, &["i3"])),
            ),
            awg_i4: Self::field(
                window,
                cx,
                t!("config-i4"),
                Some(&guided_prefill_value(params, &["i4"])),
            ),
            awg_i5: Self::field(
                window,
                cx,
                t!("config-i5"),
                Some(&guided_prefill_value(params, &["i5"])),
            ),
            awg_hpk: Self::field(
                window,
                cx,
                t!("config-header-protection-key"),
                Some(&guided_prefill_value(params, &["headerprotectionkey"])),
            ),
            awg_pad: Self::field(
                window,
                cx,
                t!("config-content-padding"),
                Some(&guided_prefill_value(params, &["contentpaddingaddition"])),
            ),
            awg_rekey_after: Self::field(
                window,
                cx,
                t!("config-rekey-after"),
                Some(&guided_prefill_value(params, &["rekeyaftertime"])),
            ),
            awg_rekey_timeout: Self::field(
                window,
                cx,
                t!("config-rekey-timeout"),
                Some(&guided_prefill_value(params, &["rekeytimeout"])),
            ),
            awg_reject_after: Self::field(
                window,
                cx,
                t!("config-reject-after"),
                Some(&guided_prefill_value(params, &["rejectaftertime"])),
            ),
            awg_keepalive_timeout: Self::field(
                window,
                cx,
                t!("config-keepalive-timeout"),
                Some(&guided_prefill_value(params, &["keepalivetimeout"])),
            ),
            awg_max_handshake: Self::field(
                window,
                cx,
                t!("config-max-handshake"),
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

            hy_server_ports: Self::field(
                window,
                cx,
                t!("config-server-ports-placeholder"),
                Some(&guided_prefill_value(params, &["server_ports"])),
            ),
            hy_hop_interval: Self::field(
                window,
                cx,
                t!("config-hop-interval-placeholder"),
                Some(&guided_prefill_value(params, &["hop_interval"])),
            ),
            hy_up: Self::field(
                window,
                cx,
                t!("config-up"),
                Some(&guided_prefill_value(params, &["up", "up_mbps", "upmbps"])),
            ),
            hy_down: Self::field(
                window,
                cx,
                t!("config-down"),
                Some(&guided_prefill_value(
                    params,
                    &["down", "down_mbps", "downmbps"],
                )),
            ),
            hy_obfs: Self::field(
                window,
                cx,
                t!("config-obfs"),
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
                t!("config-auth-secret"),
                Some(&guided_prefill_value(params, &["auth"])),
            ),
            hy_recv_conn: Self::field(
                window,
                cx,
                t!("config-recv-window-conn"),
                Some(&guided_prefill_value(params, &["recv_window_conn"])),
            ),
            hy_recv: Self::field(
                window,
                cx,
                t!("config-recv-window"),
                Some(&guided_prefill_value(params, &["recv_window"])),
            ),
            hy_mtu: Rc::new(RefCell::new(guided_prefill_bool(
                params,
                &["disable_mtu_discovery"],
            ))),
            hy_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
                Some(&guided_prefill_value(params, &["password"])),
            ),
            tuic_uuid: Self::secret_field(
                window,
                cx,
                t!("config-uuid"),
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            tuic_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
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
                t!("config-heartbeat-placeholder"),
                Some(&guided_prefill_value(params, &["heartbeat"])),
            ),

            socks_version: Self::panel_select(
                window,
                cx,
                socks_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            socks_username: Self::field(
                window,
                cx,
                t!("config-username"),
                Some(&guided_prefill_value(params, &["username"])),
            ),
            socks_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
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
            http_username: Self::field(
                window,
                cx,
                t!("config-username"),
                Some(&guided_prefill_value(params, &["username"])),
            ),
            http_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
                Some(&guided_prefill_value(params, &["password"])),
            ),
            http_path: Self::field(
                window,
                cx,
                t!("config-path"),
                Some(&guided_prefill_value(params, &["path"])),
            ),
            http_headers: Self::field(
                window,
                cx,
                t!("config-headers"),
                Some(&guided_prefill_value(params, &["headers"])),
            ),
            anytls_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
                Some(&guided_prefill_value(params, &["password"])),
            ),
            anytls_interval: Self::field(
                window,
                cx,
                t!("config-idle-check-placeholder"),
                Some(&guided_prefill_value(
                    params,
                    &["idle_session_check_interval", "interval"],
                )),
            ),
            anytls_timeout: Self::field(
                window,
                cx,
                t!("config-idle-timeout-placeholder"),
                Some(&guided_prefill_value(
                    params,
                    &["idle_session_timeout", "timeout"],
                )),
            ),
            anytls_min: Self::field(
                window,
                cx,
                t!("config-min-idle"),
                Some(&guided_prefill_value(params, &["min_idle_session", "min"])),
            ),
            naive_username: Self::field(
                window,
                cx,
                t!("config-username"),
                Some(&guided_prefill_value(params, &["username"])),
            ),
            naive_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
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
            ssh_user: Self::field(
                window,
                cx,
                t!("config-user"),
                Some(&guided_prefill_value(params, &["user", "username"])),
            ),
            ssh_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
                Some(&guided_prefill_value(params, &["password"])),
            ),
            ssh_private_key: Self::area(
                window,
                cx,
                t!("config-private-key-paste"),
                Some(&guided_prefill_value(
                    params,
                    &["private_key", "privatekey"],
                )),
            ),
            ssh_key_path: Self::field(
                window,
                cx,
                t!("config-private-key-path"),
                Some(&guided_prefill_value(params, &["private_key_path"])),
            ),
            ssh_key_pass: Self::secret_field(
                window,
                cx,
                t!("config-private-key-passphrase"),
                Some(&guided_prefill_value(
                    params,
                    &["private_key_passphrase", "private_key_pass"],
                )),
            ),
            ssh_host_key: Self::field(
                window,
                cx,
                t!("config-host-key"),
                Some(&guided_prefill_list(params, &["host_key"])),
            ),
            ssh_host_key_algs: Self::field(
                window,
                cx,
                t!("config-host-key-algs"),
                Some(&guided_prefill_list(params, &["host_key_algorithms"])),
            ),
            ssh_client_version: Self::field(
                window,
                cx,
                t!("config-client-version"),
                Some(&guided_prefill_value(params, &["client_version"])),
            ),
            snell_psk: Self::secret_field(
                window,
                cx,
                t!("config-psk"),
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
                t!("config-obfs-mode"),
                Some(&guided_prefill_value(params, &["obfs_mode", "obfs"])),
            ),
            snell_obfs_host: Self::field(
                window,
                cx,
                t!("config-obfs-host"),
                Some(&guided_prefill_value(params, &["obfs_host", "obfs-host"])),
            ),
            snell_userkey: Self::field(
                window,
                cx,
                t!("config-user-key"),
                Some(&guided_prefill_value(params, &["userkey"])),
            ),
            snell_reuse: Rc::new(RefCell::new(guided_prefill_bool(params, &["reuse"]))),
            snell_network: Self::field(
                window,
                cx,
                t!("config-network"),
                Some(&guided_prefill_value(params, &["network"])),
            ),
            snell_mode: Self::field(
                window,
                cx,
                t!("config-mode"),
                Some(&guided_prefill_value(params, &["mode"])),
            ),
            shadowtls_version: Self::panel_select(
                window,
                cx,
                shadowtls_version_choices(),
                &guided_prefill_value(params, &["version"]),
            ),
            shadowtls_password: Self::secret_field(
                window,
                cx,
                t!("config-password"),
                Some(&guided_prefill_value(params, &["password"])),
            ),
            xray_uuid: Self::secret_field(
                window,
                cx,
                t!("config-uuid"),
                Some(&guided_prefill_value(params, &["uuid", "id"])),
            ),
            xray_enc: Self::field(
                window,
                cx,
                t!("config-encryption-placeholder"),
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
                t!("config-sni"),
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
                t!("config-alpn"),
                Some(&guided_prefill_value(params, &["alpn"])),
            ),
            xray_pbk: Self::field(
                window,
                cx,
                t!("config-reality-pbk"),
                Some(&guided_prefill_value(params, &["pbk"])),
            ),
            xray_sid: Self::field(
                window,
                cx,
                t!("config-reality-sid"),
                Some(&guided_prefill_value(params, &["sid"])),
            ),
            xray_spiderx: Self::field(
                window,
                cx,
                t!("config-reality-spiderx"),
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
                t!("config-mode"),
                Some(&guided_prefill_value(params, &["mode"])),
            ),
            xray_mux: Self::panel_select(
                window,
                cx,
                xray_mux_choices(),
                &guided_prefill_value(params, &["xray_mux"]),
            ),
            chain_hops: Rc::new(RefCell::new(chain_nodes_of(params))),
            chain_pick: Self::panel_select(window, cx, chain_pick_choices(&[]), ""),
            chain_library: Rc::new(RefCell::new(Vec::new())),
            extra_exe: Self::field(
                window,
                cx,
                t!("config-executable-placeholder"),
                Some(&guided_prefill_value(params, &["exe", "binary"])),
            ),
            extra_args: Self::field(
                window,
                cx,
                t!("config-args"),
                Some(&guided_prefill_value(params, &["args"])),
            ),
            extra_config: Self::area(
                window,
                cx,
                t!("config-config-json-placeholder"),
                Some(&extra_config_prefill(params)),
            ),
            advanced: Rc::new(RefCell::new(false)),
        }
    }

    pub fn port_budget_text(rows: &[ProxyConfig]) -> String {
        use crate::store::{
            AUTO_PORT_END, AUTO_PORT_SPILL_END, AUTO_PORT_SPILL_START, AUTO_PORT_START,
        };
        let total = (AUTO_PORT_END - AUTO_PORT_START + 1) as usize
            + (AUTO_PORT_SPILL_END - AUTO_PORT_SPILL_START + 1) as usize;
        let used = rows
            .iter()
            .filter(|row| crate::store::is_auto_port(row.port))
            .count();
        t!("config-port-budget", used = used, total = total)
    }

    pub fn uuid_row(
        &self,
        view: &Entity<AppView>,
        secret: &Entity<InputState>,
        id_prefix: &str,
        error: Option<AnyElement>,
        cx: &App,
    ) -> impl IntoElement {
        let secret_entity = secret.clone();
        let uuid_view = view.clone();
        let presence = secret_state_caption(secret.read(cx).value().as_ref()).to_string();
        field().label(t!("config-uuid-required").to_string()).child(
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
                                .child(labelled_masked_input(t!("config-uuid"), secret)),
                        )
                        .child(
                            Button::new(format!("{id_prefix}-uuid-generate"))
                                .small()
                                .label(t!("config-generate"))
                                .tooltip(t!("config-generate-uuid-tip"))
                                .accessibility_label(t!("config-generate-uuid-tip"))
                                .on_click(move |_, window, cx| {
                                    let generated = generate_uuid_v4();
                                    uuid_view.update(cx, |_, cx| {
                                        secret_entity.update(cx, |state, cx| {
                                            state.set_value(generated, &mut *window, cx);
                                        });
                                    });
                                    window.push_notification(
                                        Notification::info(t!("generated-new-uuid")),
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
                )
                .children(error),
        )
    }

    pub fn panel_select_row(
        label: impl Into<String>,
        tip_id: &'static str,
        tip: impl Into<String>,
        select_id: &'static str,
        a11y: impl Into<String>,
        state: &Entity<SelectState<PanelChoiceList>>,
    ) -> Field {
        let label = label.into();
        let tip = tip.into();
        let a11y = a11y.into();
        field().label(label).child(
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

    /// K2: the merged home of everything the old `Advanced settings` dialog
    /// held (dial + TLS extras), the per-protocol `… settings [Show]` pages
    /// and the TLS / network / mux panels — one `Advanced Settings` disclosure
    /// instead of four competing doors. `None` when the protocol has nothing
    /// advanced to show.
    pub fn advanced_sections(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> Option<AnyElement> {
        let protocol = self.protocol_text(cx);
        let mut body = div().flex().flex_col().gap_2();
        let mut shown = false;
        // Network (stream transport) only ever lived on the vless/vmess/trojan
        // pages — the same set the old `has_network_panel` gate named.
        if has_network_panel(&protocol) {
            body = body.child(self.s1_network_section(view, cx));
            shown = true;
        }
        if let Some(forced) = tls_panel_plan(&protocol) {
            body = body.child(self.s2_tls_section(view, repaint, cx, forced));
            body = body.child(self.s2_extras_section(repaint));
            body = body.child(self.s2_security_section(cx));
            shown = true;
        }
        if matches!(
            protocol.as_str(),
            "vless" | "vmess" | "trojan" | "shadowsocks"
        ) {
            let vision = protocol == "vless"
                && Self::panel_value(&self.vless_flow, cx, "") == "xtls-rprx-vision";
            body = body.child(self.s3_section(view, repaint, cx, vision));
            shown = true;
        }
        if has_network_panel(&protocol) {
            if let Some(camouflage) = self.s1_camouflage_section(cx) {
                body = body.child(camouflage);
                shown = true;
            }
        }
        if has_dial_panel(&protocol) {
            body = body.child(self.s4_section(view, repaint, cx));
            shown = true;
        }
        shown.then(|| body.into_any_element())
    }

    pub fn s1_network_section(&self, view: &Entity<AppView>, _cx: &App) -> impl IntoElement {
        let network_select = self.s1_network.clone();
        let picker = field().label(t!("config-network").to_string()).child(
            div()
                .id("s1-network-tip")
                .test_support()
                .tooltip(move |_, cx| {
                    cx.new(|_| Tooltip::new(t!("config-network-tip").to_string()))
                        .into()
                })
                .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                    cx.stop_propagation();
                })
                .child(
                    Select::new(&network_select)
                        .id("s1-network")
                        .small()
                        .accessibility_label(t!("config-network-tip")),
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
                                .child(Label::new(t!("config-network"))),
                        )
                        .child(panel_toggle_button(
                            "s1-toggle",
                            if *self.s1_open.borrow() {
                                t!("config-hide")
                            } else {
                                t!("config-show")
                            },
                            view,
                            &self.s1_open,
                        )),
                )
                .content(div().id("s1-section").test_support().child(picker)),
        )
    }

    pub fn s1_camouflage_section(&self, cx: &App) -> Option<AnyElement> {
        let network = Self::panel_value(&self.s1_network, cx, "tcp");
        let conditional = |id: &'static str, label: String, state: &Entity<InputState>| {
            field()
                .label(label.clone())
                .child(labelled_input(label, state).id(id))
        };
        let mut body = div().flex().flex_col().gap_1();
        let disguised = match network.as_str() {
            "ws" => {
                body = body
                    .child(conditional("s1-path", t!("config-path"), &self.s1_path))
                    .child(conditional("s1-host", t!("config-host"), &self.s1_host));
                true
            }
            "httpupgrade" | "http" => {
                body = body
                    .child(conditional("s1-host", t!("config-host"), &self.s1_host))
                    .child(conditional("s1-path", t!("config-path"), &self.s1_path));
                if network == "http" {
                    body = body.child(conditional(
                        "s1-method",
                        t!("config-method"),
                        &self.s1_method,
                    ));
                }
                true
            }
            "grpc" => {
                body = body.child(conditional(
                    "s1-service-name",
                    t!("config-service-name"),
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
                                .child(Label::new(t!("config-section-camouflage"))),
                        )
                        .child(div().id("camouflage-section").test_support().child(body)),
                )
                .into_any_element(),
        )
    }

    pub fn s2_tls_section(
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
                        .child(t!("config-tls-forced")),
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
                            .label(t!("config-enable-tls"))
                            .tooltip(t!("config-enable-tls-tip"))
                            .accessibility_label(t!("config-enable-tls-aria"))
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
                                    if forced {
                                        t!("config-section-tls-always")
                                    } else {
                                        t!("config-section-tls")
                                    }
                                    .to_string(),
                                )),
                        )
                        .child(panel_toggle_button(
                            "s2-toggle",
                            if *self.s2_open.borrow() {
                                t!("config-hide")
                            } else {
                                t!("config-show")
                            },
                            view,
                            &self.s2_open,
                        )),
                )
                .content(div().id("s2-section").test_support().child(tls_body)),
        )
    }

    pub fn s2_security_section(&self, cx: &App) -> impl IntoElement {
        let _ = cx;
        let fp_select = self.s2_fp.clone();
        let body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                field().label(t!("config-sni").to_string()).child(
                    div()
                        .id("s2-sni")
                        .test_support()
                        .child(labelled_input(t!("config-sni"), &self.s2_sni)),
                ),
            )
            .child(
                field().label(t!("config-alpn").to_string()).child(
                    div()
                        .id("s2-alpn")
                        .test_support()
                        .child(labelled_input(t!("config-alpn"), &self.s2_alpn)),
                ),
            )
            .child(
                field().label(t!("config-fingerprint").to_string()).child(
                    div()
                        .id("s2-fp-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new(t!("config-fingerprint-tip").to_string()))
                                .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&fp_select)
                                .id("s2-fp")
                                .small()
                                .accessibility_label(t!("config-fingerprint-tip")),
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
                        .child(Label::new(t!("config-section-tls-security"))),
                )
                .child(div().id("tls-security-section").test_support().child(body)),
        )
    }

    pub fn s2_extras_section(&self, repaint: &DialogRepaint) -> impl IntoElement {
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
                            .label(t!("config-insecure"))
                            .tooltip(t!("config-insecure-tip"))
                            .accessibility_label(t!("config-insecure-aria"))
                            .on_change(move |next: &bool, window, cx| {
                                *s2_insecure.borrow_mut() = *next;
                                s2_repaint_insecure(window, cx);
                            }),
                    ),
                ),
            )
            .child(
                field()
                    .label(t!("config-reality-public-key").to_string())
                    .child(div().id("s2-pbk").test_support().child(labelled_input(
                        t!("config-reality-public-key"),
                        &self.s2_pbk,
                    ))),
            )
            .child(
                field()
                    .label(t!("config-reality-short-id").to_string())
                    .child(
                        div()
                            .id("s2-sid")
                            .test_support()
                            .child(labelled_input(t!("config-reality-short-id"), &self.s2_sid)),
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
                        .child(Label::new(t!("config-section-tls-extras"))),
                )
                .child(div().id("tls-extras-section").test_support().child(body)),
        )
    }

    pub fn s3_section(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
        vision: bool,
    ) -> AnyElement {
        let _ = cx;
        if vision {
            return field()
                .label_indent(false)
                .child(
                    div()
                        .id("s3-vision-note")
                        .test_support()
                        .child(t!("config-mux-vision")),
                )
                .into_any_element();
        }
        let effective = self.s3_effective_mode();
        let selected = ["", "on", "off"].iter().position(|mode| *mode == effective);
        let mode_labels = [t!("config-keep-default"), t!("config-on"), t!("config-off")];
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
                Radio::new(label.clone())
                    .small()
                    .label(label.clone())
                    .tooltip(t!("config-multiplex-tip", label = label)),
            );
        }
        let mut s3_body = div().flex().flex_col().gap_1().child(
            field()
                .label(t!("config-multiplex-mode").to_string())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .id("s3-mux-caption")
                                .test_support()
                                .child(Label::new(t!("config-multiplex-mode"))),
                        )
                        .child(div().id("s3-mux-group").test_support().child(group)),
                ),
        );
        if *self.s3_brutal.borrow() && effective == "on" && self.s3_mode.borrow().as_str() != "on" {
            s3_body = s3_body.child(
                div()
                    .id("s3-brutal-note")
                    .test_support()
                    .child(t!("config-brutal-mux")),
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
                            .label(t!("config-brutal"))
                            .tooltip(t!("config-brutal-tip"))
                            .accessibility_label(t!("config-brutal-aria"))
                            .on_change(move |next: &bool, window, cx| {
                                *brutal_cell.borrow_mut() = *next;
                                brutal_repaint(window, cx);
                            }),
                    ),
                ),
            )
            .child(
                field().label(t!("config-brutal-up").to_string()).child(
                    div()
                        .id("s3-up")
                        .test_support()
                        .child(labelled_input(t!("config-brutal-up"), &self.s3_up)),
                ),
            )
            .child(
                field().label(t!("config-brutal-down").to_string()).child(
                    div()
                        .id("s3-down")
                        .test_support()
                        .child(labelled_input(t!("config-brutal-down"), &self.s3_down)),
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
                                    .child(Label::new(t!("config-section-mux"))),
                            )
                            .child(panel_toggle_button(
                                "s3-toggle",
                                if *self.s3_open.borrow() {
                                    t!("config-hide")
                                } else {
                                    t!("config-show")
                                },
                                view,
                                &self.s3_open,
                            )),
                    )
                    .content(div().id("s3-section").test_support().child(s3_body)),
            )
            .into_any_element()
    }

    pub fn s4_section(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let _ = cx;
        let dial_switch = |id: &'static str,
                           switch_id: &'static str,
                           label: String,
                           tip: String,
                           cell: &Rc<RefCell<bool>>| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
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
                t!("config-reuse-address"),
                t!("config-reuse-tip"),
                &self.s4_reuse,
            ))
            .child(dial_switch(
                "s4-fastopen",
                "s4-fastopen-switch",
                t!("config-tcp-fast-open"),
                t!("config-tcp-fast-open-tip"),
                &self.s4_fastopen,
            ))
            .child(dial_switch(
                "s4-fragment",
                "s4-fragment-switch",
                t!("config-udp-fragment"),
                t!("config-udp-fragment-tip"),
                &self.s4_fragment,
            ))
            .child(dial_switch(
                "s4-multipath",
                "s4-multipath-switch",
                t!("config-tcp-multipath"),
                t!("config-tcp-multipath-tip"),
                &self.s4_multipath,
            ))
            .child(
                field()
                    .label(t!("config-connect-timeout").to_string())
                    .child(div().id("s4-timeout").test_support().child(labelled_input(
                        t!("config-connect-timeout"),
                        &self.s4_timeout,
                    ))),
            )
            .child(
                field()
                    .label(t!("config-bind-interface").to_string())
                    .child(
                        div()
                            .id("s4-bind")
                            .test_support()
                            .child(labelled_input(t!("config-bind-interface"), &self.s4_bind)),
                    ),
            )
            .child(
                field().label(t!("config-ipv4-bind").to_string()).child(
                    div()
                        .id("s4-inet4")
                        .test_support()
                        .child(labelled_input(t!("config-ipv4-bind"), &self.s4_inet4)),
                ),
            )
            .child(
                field().label(t!("config-ipv6-bind").to_string()).child(
                    div()
                        .id("s4-inet6")
                        .test_support()
                        .child(labelled_input(t!("config-ipv6-bind"), &self.s4_inet6)),
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
                        .child(Label::new(t!("config-section-dial"))),
                )
                .child(div().id("s4-section").test_support().child(s4_body)),
        )
    }

    pub fn vmess_block(
        &self,
        view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        uuid_error: Option<AnyElement>,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.vmess_uuid, "vmess", uuid_error, cx));
        block = block.child(
            field().label(t!("config-alter-id").to_string()).child(
                div()
                    .id("vmess-aid")
                    .test_support()
                    .child(labelled_input(t!("config-alter-id"), &self.vmess_aid)),
            ),
        );
        block = block.child(Self::panel_select_row(
            t!("config-security"),
            "vmess-security-tip",
            t!("config-vmess-security-tip"),
            "vmess-security",
            t!("config-vmess-security-aria"),
            &self.vmess_security,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-packet-encoding"),
            "vmess-packet-encoding-tip",
            t!("config-vmess-packet-encoding-tip"),
            "vmess-packet-encoding",
            t!("config-vmess-packet-encoding-aria"),
            &self.vmess_packet_encoding,
        ));

        block
    }

    pub fn trojan_block(
        &self,
        _view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(password_row(
            t!("config-password-required"),
            "trojan-password",
            &self.trojan_password,
            cx,
        ));

        block
    }

    pub fn shadowsocks_block(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            t!("config-method"),
            "ss-method-tip",
            t!("config-ss-method-tip"),
            "ss-method",
            t!("config-ss-method-aria"),
            &self.ss_method,
        ));
        block = block.child(password_row(
            t!("config-password-required"),
            "ss-password",
            &self.ss_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-plugin"),
            "ss-plugin-tip",
            t!("config-ss-plugin-tip"),
            "ss-plugin",
            t!("config-ss-plugin-aria"),
            &self.ss_plugin,
        ));
        block = block.child(
            field()
                .label(t!("config-plugin-options").to_string())
                .child(
                    div()
                        .id("ss-plugin-opts")
                        .test_support()
                        .child(labelled_input(
                            t!("config-plugin-options"),
                            &self.ss_plugin_opts,
                        )),
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
                        .label(t!("config-uot"))
                        .tooltip(t!("config-uot-ss-tip"))
                        .accessibility_label(t!("config-uot-ss-aria"))
                        .on_change(move |next: &bool, window, cx| {
                            *uot_cell.borrow_mut() = *next;
                            uot_repaint(window, cx);
                        }),
                ),
            ),
        );
        if !Self::panel_value(&self.ss_plugin, cx, "").trim().is_empty() {
            block = block.child(
                field().label_indent(false).child(
                    div()
                        .id("ss-plugin-note")
                        .test_support()
                        .child(t!("config-ss-plugin-note")),
                ),
            );
        }

        block
    }

    pub fn wg_peer_block(
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
        let row = |id: String, label: String, state: &Entity<InputState>| {
            field()
                .label(label.clone())
                .child(labelled_input(label, state).id(id))
        };
        let masked_row = |id: String, label: String, state: &Entity<InputState>| {
            field()
                .label(label.clone())
                .child(labelled_masked_input(label, state).id(id))
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                field()
                    .label(t!("config-private-key-required").to_string())
                    .child(
                        div().flex().flex_col().gap_1().child(
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
                                        .child(labelled_masked_input(
                                            t!("config-private-key"),
                                            peer.private_key,
                                        )),
                                )
                                .child(
                                    Button::new(format!("{prefix}-private-key-generate"))
                                        .small()
                                        .label(t!("config-generate"))
                                        .tooltip(t!("config-generate-wg-tip"))
                                        .accessibility_label(t!("config-generate-wg-aria"))
                                        .on_click(move |_, window, cx| {
                                            let generated = generate_wireguard_key();
                                            keygen_view.update(cx, |_, cx| {
                                                secret_entity.update(cx, |state, cx| {
                                                    state.set_value(generated, &mut *window, cx);
                                                });
                                            });
                                            window.push_notification(
                                                Notification::info(t!("generated-new-wg-key")),
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
                    ),
            )
            .child(row(
                format!("{prefix}-public-key"),
                t!("config-public-key-peer"),
                peer.public_key,
            ))
            .child(masked_row(
                format!("{prefix}-preshared-key"),
                t!("config-preshared-key"),
                peer.pre_shared_key,
            ))
            .child(row(
                format!("{prefix}-reserved"),
                t!("config-reserved"),
                peer.reserved,
            ))
            .child(row(
                format!("{prefix}-keepalive"),
                t!("config-keepalive"),
                peer.keepalive,
            ))
            .child(row(
                format!("{prefix}-local-addr"),
                t!("config-local-address"),
                peer.local_addr,
            ))
            .child(row(format!("{prefix}-dns"), t!("config-dns"), peer.dns))
            .child(row(format!("{prefix}-mtu"), t!("config-mtu"), peer.mtu))
            .child(row(
                format!("{prefix}-workers"),
                t!("config-workers"),
                peer.workers,
            ))
            .child(
                field().label_indent(false).child(
                    div().id(format!("{prefix}-sys-ifc")).test_support().child(
                        Switch::new(format!("{prefix}-sys-ifc-switch"))
                            .small()
                            .checked(*peer.sys_ifc.borrow())
                            .label(t!("config-system-interface"))
                            .tooltip(t!("config-system-interface-tip"))
                            .accessibility_label(t!("config-system-interface-aria"))
                            .on_change(move |next: &bool, window, cx| {
                                *sys_cell.borrow_mut() = *next;
                                sys_repaint(window, cx);
                            }),
                    ),
                ),
            )
    }

    pub fn wireguard_block(
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

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("wg-notes")
                    .test_support()
                    .child(t!("config-wg-note")),
            ),
        );
        block
    }

    pub fn amneziawg_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let row = |id: &'static str, label: String, state: &Entity<InputState>| {
            field()
                .label(label.clone())
                .child(labelled_input(label, state).id(id))
        };
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: String,
                          tip: String,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
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
        let caption =
            |id: &'static str, text: String| div().id(id).test_support().child(Label::new(text));
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
            t!("config-enable-amnezia"),
            t!("config-enable-amnezia-tip"),
            &self.awg_enable,
            repaint,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-awg-version"),
            "awg-version-tip",
            t!("config-awg-version-tip"),
            "awg-version",
            t!("config-awg-version-aria"),
            &self.awg_version,
        ));
        block = block.child(caption("awg-junk-caption", t!("config-section-awg-junk")));
        block = block
            .child(row("awg-jc", t!("config-jc"), &self.awg_jc))
            .child(row("awg-jmin", t!("config-jmin"), &self.awg_jmin))
            .child(row("awg-jmax", t!("config-jmax"), &self.awg_jmax));
        block = block.child(caption(
            "awg-length-caption",
            t!("config-section-awg-length"),
        ));
        block = block
            .child(row("awg-s1", t!("config-s1"), &self.awg_s1))
            .child(row("awg-s2", t!("config-s2"), &self.awg_s2))
            .child(row("awg-s3", t!("config-s3"), &self.awg_s3))
            .child(row("awg-s4", t!("config-s4"), &self.awg_s4));
        block = block.child(caption(
            "awg-header-caption",
            t!("config-section-awg-header"),
        ));
        block = block
            .child(row("awg-h1", t!("config-h1"), &self.awg_h1))
            .child(row("awg-h2", t!("config-h2"), &self.awg_h2))
            .child(row("awg-h3", t!("config-h3"), &self.awg_h3))
            .child(row("awg-h4", t!("config-h4"), &self.awg_h4));
        block = block.child(caption(
            "awg-chain-caption",
            t!("config-section-awg-chains"),
        ));
        block = block
            .child(row("awg-i1", t!("config-i1"), &self.awg_i1))
            .child(row("awg-i2", t!("config-i2"), &self.awg_i2))
            .child(row("awg-i3", t!("config-i3"), &self.awg_i3))
            .child(row("awg-i4", t!("config-i4"), &self.awg_i4))
            .child(row("awg-i5", t!("config-i5"), &self.awg_i5));

        if Self::panel_value(&self.awg_version, cx, "") == "3.x" {
            block = block.child(caption("awg-v3-caption", t!("config-section-awg-v3")));
            block = block
                .child(row(
                    "awg-hpk",
                    t!("config-header-protection-key"),
                    &self.awg_hpk,
                ))
                .child(row("awg-pad", t!("config-content-padding"), &self.awg_pad))
                .child(row(
                    "awg-rekey-after",
                    t!("config-rekey-after"),
                    &self.awg_rekey_after,
                ))
                .child(row(
                    "awg-rekey-timeout",
                    t!("config-rekey-timeout"),
                    &self.awg_rekey_timeout,
                ))
                .child(row(
                    "awg-reject-after",
                    t!("config-reject-after"),
                    &self.awg_reject_after,
                ))
                .child(row(
                    "awg-keepalive-timeout",
                    t!("config-keepalive-timeout"),
                    &self.awg_keepalive_timeout,
                ))
                .child(row(
                    "awg-max-handshake",
                    t!("config-max-handshake"),
                    &self.awg_max_handshake,
                ));
            block = block.child(switch_row(
                "awg-trailers",
                "awg-trailers-switch".to_string(),
                t!("config-awg-trailers"),
                t!("config-awg-trailers-tip"),
                &self.awg_trailers,
                repaint,
            ));
            block = block.child(switch_row(
                "awg-cookies",
                "awg-cookies-switch".to_string(),
                t!("config-awg-cookies"),
                t!("config-awg-cookies-tip"),
                &self.awg_cookies,
                repaint,
            ));
        } else {
            block = block.child(
                field().label_indent(false).child(
                    div()
                        .id("awg-v3-note")
                        .test_support()
                        .child(t!("config-awg-v3-hidden")),
                ),
            );
        }

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("awg-notes")
                    .test_support()
                    .child(t!("config-awg-note")),
            ),
        );
        block
    }

    pub fn hysteria_block(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let row = |id: &'static str, label: String, state: &Entity<InputState>| {
            field()
                .label(label.clone())
                .child(labelled_input(label, state).id(id))
        };
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: String,
                          tip: String,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
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
                field()
                    .label(t!("config-protocol-version").to_string())
                    .child(
                        div()
                            .id("hy-version-locked")
                            .test_support()
                            .child(if is_v2 {
                                t!("config-hy-version-locked-2")
                            } else {
                                t!("config-hy-version-locked-1")
                            }),
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
                        .tooltip(t!("config-hy-version-choice-tip", label = label)),
                );
            }
            block = block.child(
                field()
                    .label(t!("config-protocol-version").to_string())
                    .child(
                        div()
                            .id("hy-version-tip")
                            .test_support()
                            .tooltip(move |_, cx| {
                                cx.new(|_| Tooltip::new(t!("config-hy-version-tip").to_string()))
                                    .into()
                            })
                            .child(div().id("hy-version-group").test_support().child(group)),
                    ),
            );
        }
        block = block
            .child(row(
                "hy-server-ports",
                t!("config-server-ports-placeholder"),
                &self.hy_server_ports,
            ))
            .child(row(
                "hy-hop-interval",
                t!("config-hop-interval-placeholder"),
                &self.hy_hop_interval,
            ))
            .child(row("hy-up", t!("config-up"), &self.hy_up))
            .child(row("hy-down", t!("config-down"), &self.hy_down))
            .child(row("hy-obfs", t!("config-obfs"), &self.hy_obfs));
        if is_v2 {
            block = block.child(password_row(
                t!("config-password-required"),
                "hy-password",
                &self.hy_password,
                cx,
            ));
        } else {
            block = block.child(Self::panel_select_row(
                t!("config-auth-type"),
                "hy-auth-type-tip",
                t!("config-hy-auth-type-tip"),
                "hy-auth-type",
                t!("config-hy-auth-type-aria"),
                &self.hy_auth_type,
            ));
            block = block.child(password_row(
                t!("config-auth-required"),
                "hy-auth",
                &self.hy_auth,
                cx,
            ));
            block = block
                .child(row(
                    "hy-recv-window-conn",
                    t!("config-recv-window-conn"),
                    &self.hy_recv_conn,
                ))
                .child(row(
                    "hy-recv-window",
                    t!("config-recv-window"),
                    &self.hy_recv,
                ));
            block = block.child(switch_row(
                "hy-mtu",
                "hy-mtu-switch".to_string(),
                t!("config-hy-mtu"),
                t!("config-hy-mtu-tip"),
                &self.hy_mtu,
                repaint,
            ));
        }

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("hy-notes")
                    .test_support()
                    .child(t!("config-hy-note")),
            ),
        );
        block
    }

    pub fn tuic_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        uuid_error: Option<AnyElement>,
        cx: &App,
    ) -> impl IntoElement {
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: String,
                          tip: String,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
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
        block = block.child(self.uuid_row(view, &self.tuic_uuid, "tuic", uuid_error, cx));
        block = block.child(password_row(
            t!("config-password-required"),
            "tuic-password",
            &self.tuic_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-congestion"),
            "tuic-cc-tip",
            t!("config-tuic-cc-tip"),
            "tuic-cc",
            t!("config-tuic-cc-aria"),
            &self.tuic_cc,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-udp-relay"),
            "tuic-relay-tip",
            t!("config-tuic-relay-tip"),
            "tuic-relay",
            t!("config-tuic-relay-aria"),
            &self.tuic_relay,
        ));
        block = block.child(switch_row(
            "tuic-uot",
            "tuic-uot-switch".to_string(),
            t!("config-tuic-uot"),
            t!("config-tuic-uot-tip"),
            &self.tuic_uot,
            repaint,
        ));
        block = block.child(switch_row(
            "tuic-zero-rtt",
            "tuic-zero-rtt-switch".to_string(),
            t!("config-tuic-zero-rtt"),
            t!("config-tuic-zero-rtt-tip"),
            &self.tuic_zero_rtt,
            repaint,
        ));
        block = block.child(
            field().label(t!("config-heartbeat").to_string()).child(
                div()
                    .id("tuic-heartbeat")
                    .test_support()
                    .child(labelled_input(t!("config-heartbeat"), &self.tuic_heartbeat)),
            ),
        );

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("tuic-notes")
                    .test_support()
                    .child(t!("config-tuic-note")),
            ),
        );
        block
    }

    pub fn socks_block(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let uot_cell = self.socks_uot.clone();
        let uot_repaint = repaint.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            t!("config-version"),
            "socks-version-tip",
            t!("config-socks-version-tip"),
            "socks-version",
            t!("config-socks-version-aria"),
            &self.socks_version,
        ));
        block = block.child(
            field().label(t!("config-username").to_string()).child(
                div()
                    .id("socks-username")
                    .test_support()
                    .child(labelled_input(t!("config-username"), &self.socks_username)),
            ),
        );
        block = block.child(password_row(
            t!("config-password"),
            "socks-password",
            &self.socks_password,
            cx,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-network"),
            "socks-network-tip",
            t!("config-socks-network-tip"),
            "socks-network",
            t!("config-socks-network-aria"),
            &self.socks_network,
        ));
        block = block.child(
            field().label_indent(false).child(
                div().id("socks-uot").test_support().child(
                    Switch::new("socks-uot-switch")
                        .small()
                        .checked(*self.socks_uot.borrow())
                        .label(t!("config-uot"))
                        .tooltip(t!("config-uot-socks-tip"))
                        .accessibility_label(t!("config-uot-socks-aria"))
                        .on_change(move |next: &bool, window, cx| {
                            *uot_cell.borrow_mut() = *next;
                            uot_repaint(window, cx);
                        }),
                ),
            ),
        );

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("socks-notes")
                    .test_support()
                    .child(t!("config-socks-note")),
            ),
        );
        block
    }

    pub fn http_block(
        &self,
        _view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(
                field().label(t!("config-username").to_string()).child(
                    div()
                        .id("http-username")
                        .test_support()
                        .child(labelled_input(t!("config-username"), &self.http_username)),
                ),
            )
            .child(password_row(
                t!("config-password"),
                "http-password",
                &self.http_password,
                cx,
            ))
            .child(
                field().label(t!("config-path").to_string()).child(
                    div()
                        .id("http-path")
                        .test_support()
                        .child(labelled_input(t!("config-path"), &self.http_path)),
                ),
            )
            .child(
                field().label(t!("config-headers").to_string()).child(
                    div()
                        .id("http-headers")
                        .test_support()
                        .child(labelled_input(t!("config-headers"), &self.http_headers)),
                ),
            );

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("http-notes")
                    .test_support()
                    .child(t!("config-http-note")),
            ),
        );
        block
    }

    pub fn anytls_block(
        &self,
        _view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(password_row(
            t!("config-password-required"),
            "anytls-password",
            &self.anytls_password,
            cx,
        ));
        block = block
            .child(
                field().label(t!("config-idle-check").to_string()).child(
                    div()
                        .id("anytls-interval")
                        .test_support()
                        .child(labelled_input(
                            t!("config-idle-check"),
                            &self.anytls_interval,
                        )),
                ),
            )
            .child(
                field().label(t!("config-idle-timeout").to_string()).child(
                    div()
                        .id("anytls-timeout")
                        .test_support()
                        .child(labelled_input(
                            t!("config-idle-timeout"),
                            &self.anytls_timeout,
                        )),
                ),
            )
            .child(
                field().label(t!("config-min-idle").to_string()).child(
                    div()
                        .id("anytls-min")
                        .test_support()
                        .child(labelled_input(t!("config-min-idle"), &self.anytls_min)),
                ),
            );

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("anytls-notes")
                    .test_support()
                    .child(t!("config-anytls-note")),
            ),
        );
        block
    }

    pub fn naive_block(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let switch_row = |id: &'static str,
                          switch_id: String,
                          label: String,
                          tip: String,
                          cell: &Rc<RefCell<bool>>,
                          repaint: &DialogRepaint| {
            let switch_cell = cell.clone();
            let switch_repaint = repaint.clone();
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
                field()
                    .label(t!("config-username-required").to_string())
                    .child(
                        div()
                            .id("naive-username")
                            .test_support()
                            .child(labelled_input(t!("config-username"), &self.naive_username)),
                    ),
            )
            .child(password_row(
                t!("config-password-required"),
                "naive-password",
                &self.naive_password,
                cx,
            ));
        block = block.child(switch_row(
            "naive-uot",
            "naive-uot-switch".to_string(),
            t!("config-uot"),
            t!("config-uot-naive-tip"),
            &self.naive_uot,
            repaint,
        ));
        block = block.child(switch_row(
            "naive-quic",
            "naive-quic-switch".to_string(),
            t!("config-naive-quic"),
            t!("config-naive-quic-tip"),
            &self.naive_quic,
            repaint,
        ));
        block = block.child(Self::panel_select_row(
            t!("config-congestion"),
            "naive-cc-tip",
            t!("config-naive-cc-tip"),
            "naive-cc",
            t!("config-naive-cc-aria"),
            &self.naive_cc,
        ));

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("naive-notes")
                    .test_support()
                    .child(t!("config-naive-note")),
            ),
        );
        block
    }

    pub fn ssh_block(
        &self,
        view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let browse_view = view.clone();
        let browse_path = self.ssh_key_path.clone();
        let browse_key = self.ssh_private_key.clone();
        let mut block = div().flex().flex_col().gap_2();
        block =
            block
                .child(
                    field().label(t!("config-user").to_string()).child(
                        div()
                            .id("ssh-user")
                            .test_support()
                            .child(labelled_input(t!("config-user"), &self.ssh_user)),
                    ),
                )
                .child(password_row(
                    t!("config-password"),
                    "ssh-password",
                    &self.ssh_password,
                    cx,
                ))
                .child(
                    field().label(t!("config-private-key").to_string()).child(
                        div().id("ssh-private-key").test_support().child(
                            labelled_textarea(t!("config-private-key"), &self.ssh_private_key)
                                .h(px(96.)),
                        ),
                    ),
                )
                .child(
                    field()
                        .label(t!("config-private-key-path").to_string())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(div().flex_1().id("ssh-key-path").test_support().child(
                                    labelled_input(
                                        t!("config-private-key-path"),
                                        &self.ssh_key_path,
                                    ),
                                ))
                                .child(
                                    Button::new("ssh-key-browse")
                                        .small()
                                        .label(t!("config-browse"))
                                        .tooltip(t!("config-ssh-browse-tip"))
                                        .accessibility_label(t!("config-ssh-browse-aria"))
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
                    field()
                        .label(t!("config-private-key-passphrase").to_string())
                        .child(div().id("ssh-key-pass").test_support().child(
                            labelled_masked_input(
                                t!("config-private-key-passphrase"),
                                &self.ssh_key_pass,
                            ),
                        )),
                )
                .child(
                    field().label(t!("config-host-key").to_string()).child(
                        div()
                            .id("ssh-host-key")
                            .test_support()
                            .child(labelled_input(t!("config-host-key"), &self.ssh_host_key)),
                    ),
                )
                .child(
                    field().label(t!("config-host-key-algs").to_string()).child(
                        div()
                            .id("ssh-host-key-algs")
                            .test_support()
                            .child(labelled_input(
                                t!("config-host-key-algs"),
                                &self.ssh_host_key_algs,
                            )),
                    ),
                )
                .child(
                    field()
                        .label(t!("config-client-version").to_string())
                        .child(div().id("ssh-client-version").test_support().child(
                            labelled_input(t!("config-client-version"), &self.ssh_client_version),
                        )),
                );

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("ssh-notes")
                    .test_support()
                    .child(t!("config-ssh-note")),
            ),
        );
        block
    }

    pub fn snell_block(
        &self,
        _view: &Entity<AppView>,
        repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let reuse_cell = self.snell_reuse.clone();
        let reuse_repaint = repaint.clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(password_row(
                t!("config-psk-required"),
                "snell-psk",
                &self.snell_psk,
                cx,
            ))
            .child(Self::panel_select_row(
                t!("config-version"),
                "snell-version-tip",
                t!("config-snell-version-tip"),
                "snell-version",
                t!("config-snell-version-aria"),
                &self.snell_version,
            ))
            .child(
                field().label(t!("config-obfs-mode").to_string()).child(
                    div()
                        .id("snell-obfs-mode")
                        .test_support()
                        .child(labelled_input(
                            t!("config-obfs-mode"),
                            &self.snell_obfs_mode,
                        )),
                ),
            )
            .child(
                field().label(t!("config-obfs-host").to_string()).child(
                    div()
                        .id("snell-obfs-host")
                        .test_support()
                        .child(labelled_input(
                            t!("config-obfs-host"),
                            &self.snell_obfs_host,
                        )),
                ),
            )
            .child(
                field().label(t!("config-user-key").to_string()).child(
                    div()
                        .id("snell-userkey")
                        .test_support()
                        .child(labelled_input(t!("config-user-key"), &self.snell_userkey)),
                ),
            )
            .child(
                field().label(t!("config-network").to_string()).child(
                    div()
                        .id("snell-network")
                        .test_support()
                        .child(labelled_input(t!("config-network"), &self.snell_network)),
                ),
            )
            .child(
                field().label(t!("config-mode").to_string()).child(
                    div()
                        .id("snell-mode")
                        .test_support()
                        .child(labelled_input(t!("config-mode"), &self.snell_mode)),
                ),
            );
        block = block.child(
            field().label_indent(false).child(
                div().id("snell-reuse").test_support().child(
                    Switch::new("snell-reuse-switch")
                        .small()
                        .checked(*self.snell_reuse.borrow())
                        .label(t!("config-reuse"))
                        .tooltip(t!("config-snell-reuse-tip"))
                        .accessibility_label(t!("config-snell-reuse-aria"))
                        .on_change(move |next: &bool, window, cx| {
                            *reuse_cell.borrow_mut() = *next;
                            reuse_repaint(window, cx);
                        }),
                ),
            ),
        );

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("snell-notes")
                    .test_support()
                    .child(t!("config-snell-note")),
            ),
        );
        block
    }

    pub fn shadowtls_block(
        &self,
        _view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();
        block = block
            .child(Self::panel_select_row(
                t!("config-version"),
                "shadowtls-version-tip",
                t!("config-shadowtls-version-tip"),
                "shadowtls-version",
                t!("config-shadowtls-version-aria"),
                &self.shadowtls_version,
            ))
            .child(password_row(
                t!("config-password"),
                "shadowtls-password",
                &self.shadowtls_password,
                cx,
            ));

        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("shadowtls-notes")
                    .test_support()
                    .child(t!("config-shadowtls-note")),
            ),
        );
        block
    }

    pub fn xray_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        uuid_error: Option<AnyElement>,
        _cx: &App,
    ) -> impl IntoElement {
        let _ = repaint;
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(self.uuid_row(view, &self.xray_uuid, "xray", uuid_error, _cx));
        block = block
            .child(
                field().label(t!("config-encryption").to_string()).child(
                    div()
                        .id("xray-enc")
                        .test_support()
                        .child(labelled_input(t!("config-encryption"), &self.xray_enc)),
                ),
            )
            .child(Self::panel_select_row(
                t!("config-flow"),
                "xray-flow-tip",
                t!("config-xray-flow-tip"),
                "xray-flow",
                t!("config-xray-flow-aria"),
                &self.xray_flow,
            ))
            .child(
                field().label(t!("config-sni").to_string()).child(
                    div()
                        .id("xray-sni")
                        .test_support()
                        .child(labelled_input(t!("config-sni"), &self.xray_sni)),
                ),
            )
            .child(Self::panel_select_row(
                t!("config-fingerprint"),
                "xray-fp-tip",
                t!("config-xray-fp-tip"),
                "xray-fp",
                t!("config-xray-fp-aria"),
                &self.xray_fp,
            ))
            .child(
                field().label(t!("config-alpn").to_string()).child(
                    div()
                        .id("xray-alpn")
                        .test_support()
                        .child(labelled_input(t!("config-alpn"), &self.xray_alpn)),
                ),
            )
            .child(
                field().label(t!("config-reality-pbk").to_string()).child(
                    div()
                        .id("xray-pbk")
                        .test_support()
                        .child(labelled_input(t!("config-reality-pbk"), &self.xray_pbk)),
                ),
            )
            .child(
                field().label(t!("config-reality-sid").to_string()).child(
                    div()
                        .id("xray-sid")
                        .test_support()
                        .child(labelled_input(t!("config-reality-sid"), &self.xray_sid)),
                ),
            )
            .child(
                field()
                    .label(t!("config-reality-spiderx").to_string())
                    .child(
                        div()
                            .id("xray-spiderx")
                            .test_support()
                            .child(labelled_input(
                                t!("config-reality-spiderx"),
                                &self.xray_spiderx,
                            )),
                    ),
            )
            .child(Self::panel_select_row(
                t!("config-network"),
                "xray-network-tip",
                t!("config-xray-network-tip"),
                "xray-network",
                t!("config-xray-network-aria"),
                &self.xray_network,
            ))
            .child(
                field().label(t!("config-mode").to_string()).child(
                    div()
                        .id("xray-mode")
                        .test_support()
                        .child(labelled_input(t!("config-mode"), &self.xray_mode)),
                ),
            )
            .child(Self::panel_select_row(
                t!("config-section-mux"),
                "xray-mux-tip",
                t!("config-xray-mux-tip"),
                "xray-mux",
                t!("config-xray-mux-aria"),
                &self.xray_mux,
            ));
        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("xray-notes")
                    .test_support()
                    .child(t!("config-xray-note")),
            ),
        );
        block
    }

    pub fn chain_block(
        &self,
        view: &Entity<AppView>,
        repaint: &DialogRepaint,
        _cx: &App,
    ) -> impl IntoElement {
        let hops = self.chain_hops.borrow().clone();
        let library = self.chain_library.borrow().clone();
        let mut block = div().flex().flex_col().gap_2();
        block = block.child(Self::panel_select_row(
            t!("config-chain-pick"),
            "chain-pick-tip",
            t!("config-chain-pick-tip"),
            "chain-pick",
            t!("config-chain-pick-aria"),
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
                            .label(t!("config-add-hop"))
                            .tooltip(t!("config-add-hop-tip"))
                            .accessibility_label(t!("config-add-hop-aria"))
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
                            .title(EmptyTitle::new().child(t!("config-chain-empty-title")))
                            .description(
                                EmptyDescription::new().child(t!("config-chain-empty-desc")),
                            ),
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
                                .label(t!("config-replace"))
                                .tooltip(t!("config-replace-tip"))
                                .accessibility_label(t!(
                                    "config-replace-hop-aria",
                                    number = (index + 1) as i64
                                ))
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
                                .label(t!("config-remove"))
                                .tooltip(t!("config-remove-tip"))
                                .accessibility_label(t!(
                                    "config-remove-hop-aria",
                                    number = (index + 1) as i64
                                ))
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
        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("chain-notes")
                    .test_support()
                    .child(t!("config-chain-note")),
            ),
        );
        block
    }

    pub fn extra_block(
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
                    .label(t!("config-executable-placeholder").to_string())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1().id("extra-exe").test_support().child(
                                labelled_input(
                                    t!("config-executable-placeholder"),
                                    &self.extra_exe,
                                ),
                            ))
                            .child(
                                Button::new("extra-exe-browse")
                                    .small()
                                    .label(t!("config-browse"))
                                    .tooltip(t!("config-extra-browse-tip"))
                                    .accessibility_label(t!("config-extra-browse-aria"))
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
                field().label(t!("config-args").to_string()).child(
                    div()
                        .id("extra-args")
                        .test_support()
                        .child(labelled_input(t!("config-args"), &self.extra_args)),
                ),
            )
            .child(field().label(t!("config-config-label").to_string()).child(
                div().id("extra-config").test_support().child(
                    labelled_textarea(t!("config-config-label"), &self.extra_config).h(px(96.)),
                ),
            ));
        block = block.child(
            field().label_indent(false).child(
                div()
                    .id("extra-notes")
                    .test_support()
                    .child(t!("config-extra-note")),
            ),
        );
        block
    }

    pub fn vless_block(
        &self,
        view: &Entity<AppView>,
        _repaint: &DialogRepaint,
        uuid_error: Option<AnyElement>,
        cx: &App,
    ) -> impl IntoElement {
        let mut block = div().flex().flex_col().gap_2();

        block = block.child(self.uuid_row(view, &self.vless_uuid, "vless", uuid_error, cx));
        let flow_select = self.vless_flow.clone();
        let packet_select = self.vless_packet_encoding.clone();
        block = block.child(
            field().label(t!("config-flow").to_string()).child(
                div()
                    .id("vless-flow-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Tooltip::new(t!("config-vless-flow-tip").to_string()))
                            .into()
                    })
                    .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                    })
                    .child(
                        Select::new(&flow_select)
                            .id("vless-flow")
                            .small()
                            .accessibility_label(t!("config-vless-flow-aria")),
                    ),
            ),
        );
        block = block.child(
            field()
                .label(t!("config-packet-encoding").to_string())
                .child(
                    div()
                        .id("vless-packet-encoding-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                Tooltip::new(t!("config-vless-packet-encoding-aria").to_string())
                            })
                            .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&packet_select)
                                .id("vless-packet-encoding")
                                .small()
                                .accessibility_label(t!("config-vless-packet-encoding-aria")),
                        ),
                ),
        );

        block
    }

    pub fn element(
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

        let repaint_view = view.clone();
        let repaint: DialogRepaint = Rc::new(move |_window, cx| {
            repaint_view.update(cx, |_, cx| cx.notify());
        });
        let advanced_open = *self.advanced.borrow();

        // K2: the protocol-specific rows render directly. The stream-transport
        // / TLS / mux / camouflage / dial panels left this match and moved into
        // the single `Advanced Settings` disclosure further down.
        let uuid_error = local_field_error("uuid-field-error", form_error.as_ref(), "uuid", cx);
        let protocol_section = match protocol_text.as_str() {
            "vless" => Some(
                self.vless_block(&view, &repaint, uuid_error, cx)
                    .into_any_element(),
            ),
            "vmess" => Some(
                self.vmess_block(&view, &repaint, uuid_error, cx)
                    .into_any_element(),
            ),
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
            "tuic" => Some(
                self.tuic_block(&view, &repaint, uuid_error, cx)
                    .into_any_element(),
            ),

            "socks" => Some(self.socks_block(&view, &repaint, cx).into_any_element()),
            "http" | "https" => Some(self.http_block(&view, &repaint, cx).into_any_element()),
            "anytls" => Some(self.anytls_block(&view, &repaint, cx).into_any_element()),
            "naive" => Some(self.naive_block(&view, &repaint, cx).into_any_element()),
            "ssh" => Some(self.ssh_block(&view, &repaint, cx).into_any_element()),

            "snell" => Some(self.snell_block(&view, &repaint, cx).into_any_element()),
            "shadowtls" => Some(self.shadowtls_block(&view, &repaint, cx).into_any_element()),
            "xray" => Some(
                self.xray_block(&view, &repaint, uuid_error, cx)
                    .into_any_element(),
            ),
            "chain" => Some(self.chain_block(&view, &repaint, cx).into_any_element()),
            "extra-core" => Some(self.extra_block(&view, &repaint, cx).into_any_element()),
            _ => None,
        };
        let advanced_panel = self.advanced_sections(&view, &repaint, cx);

        // K2 removes `Advanced (raw JSON)` — the editor never shows a raw JSON
        // textarea for a config the guided rows can author. It survives only
        // where there is no guided block at all (noise, custom-outbound,
        // custom-config, direct, mieru, juicity, trusttunnel): for those the
        // params JSON is the whole authoring surface.
        let params_row = div()
            .id("field-params")
            .test_support()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().child(Label::new(t!("config-params"))))
            .child(labelled_textarea(t!("config-params"), &self.params).h(px(96.)))
            .child({
                let format_view = view.clone();
                let format_params = self.params.clone();
                let template_view = view.clone();
                let template_params = self.params.clone();
                let template_form = self.clone();
                let validate_view = view.clone();
                let validate_params = self.params.clone();
                let validate_form = self.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("params-format")
                            .label(t!("config-format-json"))
                            .on_click(move |_, window, cx| {
                                format_view.update(cx, |view, cx| {
                                    view.format_dialog_params(&format_params, window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("params-template")
                            .label(t!("config-load-template"))
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
                    .child(
                        Button::new("params-validate")
                            .label(t!("config-validate"))
                            .on_click(move |_, window, cx| {
                                validate_view.update(cx, |view, cx| {
                                    let protocol = validate_form.protocol_text(cx);
                                    view.validate_dialog_params(
                                        &validate_params,
                                        &protocol,
                                        window,
                                        cx,
                                    );
                                });
                            }),
                    )
            });

        let mut form = v_form().small();

        let common_head = div().flex().flex_row().items_center().gap_2().child(
            div()
                .flex_1()
                .id("common-caption")
                .test_support()
                .child(Label::new(t!("config-section-common"))),
        );
        let mut common = div().flex().flex_col().gap_1().child(common_head).child(
            field().label(t!("config-name-required").to_string()).child(
                div()
                    .id("field-name-tip")
                    .test_support()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Tooltip::new(t!("config-name-tip").to_string()))
                            .into()
                    })
                    .child(
                        div()
                            .id("field-name")
                            .test_support()
                            .child(labelled_input(t!("config-name"), &self.name)),
                    ),
            ),
        );

        if self.protocol_locked {
            let locked = self.locked_protocol.clone();
            common = common.child(
                field().label(t!("config-protocol").to_string()).child(
                    div()
                        .id("protocol-lock")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new(t!("config-protocol-locked-tip").to_string()))
                                .into()
                        })
                        .child(Label::new(t!("config-protocol-locked", protocol = locked))),
                ),
            );
        } else {
            let select = self.protocol_select.clone();
            common = common.child(
                field().label(t!("config-protocol").to_string()).child(
                    div()
                        .id("protocol-select-tip")
                        .test_support()
                        .tooltip(move |_, cx| {
                            cx.new(|_| Tooltip::new(t!("config-protocol-select-tip").to_string()))
                                .into()
                        })
                        .on_action(|_: &Confirm, _: &mut Window, cx: &mut App| {
                            cx.stop_propagation();
                        })
                        .child(
                            Select::new(&select)
                                .id("protocol-select")
                                .small()
                                .accessibility_label(t!("config-protocol")),
                        ),
                ),
            );
        }

        if !virtual_row {
            common = common.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            field().label(t!("config-server-required")).child(
                                div()
                                    .id("field-guided-server-tip")
                                    .test_support()
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| {
                                            Tooltip::new(t!("config-server-tip").to_string())
                                        })
                                        .into()
                                    })
                                    .child(
                                        div()
                                            .id("field-guided-server")
                                            .test_support()
                                            .child(labelled_input(
                                                t!("config-server"),
                                                &self.guided_server,
                                            ))
                                            .children(local_field_error(
                                                "field-guided-server-error",
                                                form_error.as_ref(),
                                                "guided_server",
                                                cx,
                                            )),
                                    ),
                            ),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            field().label(t!("config-server-port-required")).child(
                                div()
                                    .id("field-guided-port-tip")
                                    .test_support()
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| {
                                            Tooltip::new(t!("config-server-port-tip").to_string())
                                        })
                                        .into()
                                    })
                                    .child(
                                        div()
                                            .id("field-guided-port")
                                            .test_support()
                                            .child(labelled_input(
                                                t!("config-server-port"),
                                                &self.guided_port,
                                            ))
                                            .children(local_field_error(
                                                "field-guided-port-error",
                                                form_error.as_ref(),
                                                "guided_port",
                                                cx,
                                            )),
                                    ),
                            ),
                        ),
                    ),
            );
        }

        if guided.is_some_and(|guided| guided.key.is_some()) && !block_owns_identity(&protocol_text)
        {
            let identity_presence =
                secret_state_caption(self.guided_identity.read(cx).value().as_ref()).to_string();
            common = common.child(
                field()
                    .label(t!(
                        "config-identity-required",
                        label = guided_identity_label(&protocol_text)
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().id("field-guided-identity").test_support().child(
                                labelled_masked_input(
                                    guided_identity_label(&protocol_text),
                                    &self.guided_identity,
                                ),
                            ))
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
        // K2: the hint line is a plain line now — the two buttons it used
        // to carry (Advanced settings dialog, Advanced (raw JSON)) are gone.
        if let Some(hint) = guided_hint_line(&protocol_text) {
            common = common.child(
                field().label_indent(false).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .id("guided-hint")
                        .test_support()
                        .child(hint),
                ),
            );
        }
        form = form.child(
            field()
                .label_indent(false)
                .child(div().id("common-section").test_support().child(common)),
        );
        let guided_block = protocol_section.is_some();
        // F5: identity sits under Common, above Advanced — expanding Advanced
        // must never push UUID/Flow/Packet out of sight.
        if let Some(section) = protocol_section {
            form = form.child(
                field()
                    .label_indent(false)
                    .child(div().id("protocol-section").test_support().child(section)),
            );
        }
        // The raw params row only exists where no guided block can author the
        // row (see above) — every guided protocol is textarea-free.
        if !guided_block {
            form = form.child(field().label_indent(false).child(params_row));
        }
        // K2: ONE full-width `Advanced Settings` expander — the merged home of
        // the old Advanced settings dialog, the per-protocol `… settings`
        // Show pages and the TLS/transport/camouflage/dial panels. Collapsed
        // by default.
        if has_advanced_dialog(&protocol_text) {
            if let Some(panels) = advanced_panel {
                let toggle_view = view.clone();
                let toggle_cell = self.advanced.clone();
                form = form.child(
                    field().label_indent(false).child(
                        Collapsible::new()
                            .open(advanced_open)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .id("advanced-settings-caption")
                                            .test_support()
                                            .child(Label::new(t!("config-section-advanced"))),
                                    )
                                    .child(panel_toggle_button(
                                        "advanced-settings",
                                        if advanced_open {
                                            t!("config-hide")
                                        } else {
                                            t!("config-show")
                                        },
                                        &toggle_view,
                                        &toggle_cell,
                                    )),
                            )
                            .content(
                                div()
                                    .id("advanced-settings-section")
                                    .test_support()
                                    .child(panels),
                            ),
                    ),
                );
            }
        }
        // K2: Group / Bind / Port ride one compact secondary row above the
        // footer instead of three full-width rows.
        form = form.child(
            field().label_indent(false).child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-group",
                        t!("config-group"),
                        self.group,
                    )))
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-bind",
                        t!("config-bind"),
                        self.bind,
                    )))
                    .child(div().flex_1().min_w_0().child(dialog_field(
                        "field-port",
                        t!("config-port"),
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
                t!("config-test-url"),
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
                        .text_sm()
                        .text_color(warning_text_color(cx.theme()))
                        .child(warning),
                ),
            );
        }
        if let Some((field_name, message)) = form_error {
            // Guided fields carry their error beside themselves; the foot line
            // stays for everything else (params, port, test_url). The message
            // already says what is wrong, so the raw field key stays out of
            // the user's sentence.
            if !matches!(
                field_name.as_str(),
                "uuid" | "guided_server" | "guided_port"
            ) {
                form = form.child(
                    field().label_indent(false).child(
                        div()
                            .id("form-field-error")
                            .test_support()
                            .text_sm()
                            .text_color(danger_text_color(cx.theme()))
                            .child(message),
                    ),
                );
            }
        }
        form
    }

    pub fn focus_field(&self, field: &str, window: &mut Window, cx: &mut App) {
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
            "guided_server" => Some(AnyInputState::from(self.guided_server.clone())),
            "guided_port" => Some(AnyInputState::from(self.guided_port.clone())),
            "uuid" => Some(AnyInputState::from(match self.protocol_text(cx).as_str() {
                "vmess" => self.vmess_uuid.clone(),
                "tuic" => self.tuic_uuid.clone(),
                "xray" => self.xray_uuid.clone(),
                _ => self.vless_uuid.clone(),
            })),
            _ => None,
        };
        if let Some(state) = target {
            let handle = state.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    pub fn as_new(&self, cx: &App) -> NewConfig {
        NewConfig {
            name: self.name.read(cx).value().to_string(),
            protocol: self.protocol_text(cx),
            params: Secret::new(self.effective_params(cx)),
            group: self.group.read(cx).value().to_string(),
            bind_ip: self.bind.read(cx).value().to_string(),
            port: self.port.read(cx).value().to_string(),
        }
    }

    pub fn effective_params(&self, cx: &App) -> String {
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

    pub fn panel_value(
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

    pub fn vless_inner_values(&self, cx: &App) -> (String, String, String) {
        (
            self.vless_uuid.read(cx).value().to_string(),
            Self::panel_value(&self.vless_flow, cx, ""),
            Self::panel_value(&self.vless_packet_encoding, cx, ""),
        )
    }

    pub fn vmess_inner_values(&self, cx: &App) -> (String, String, String, String) {
        (
            self.vmess_uuid.read(cx).value().to_string(),
            self.vmess_aid.read(cx).value().to_string(),
            Self::panel_value(&self.vmess_security, cx, "auto"),
            Self::panel_value(&self.vmess_packet_encoding, cx, ""),
        )
    }

    pub fn ss_inner_values(&self, cx: &App) -> (String, String, String, String, bool) {
        (
            Self::panel_value(&self.ss_method, cx, ""),
            self.ss_password.read(cx).value().to_string(),
            Self::panel_value(&self.ss_plugin, cx, ""),
            self.ss_plugin_opts.read(cx).value().to_string(),
            *self.ss_uot.borrow(),
        )
    }

    pub fn wireguard_fold(&self, cx: &App, page: &str) -> WireguardFold {
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

    pub fn amnezia_fold(&self, cx: &App) -> AmneziaFold {
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

    pub fn hysteria_fold(&self, cx: &App) -> HysteriaFold {
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

    pub fn tuic_fold(&self, cx: &App) -> TuicFold {
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

    pub fn socks_fold(&self, cx: &App) -> SocksFold {
        SocksFold {
            version: Self::panel_value(&self.socks_version, cx, ""),
            username: self.socks_username.read(cx).value().to_string(),
            password: self.socks_password.read(cx).value().to_string(),
            network: Self::panel_value(&self.socks_network, cx, "tcp"),
            udp_over_tcp: *self.socks_uot.borrow(),
        }
    }

    pub fn http_fold(&self, cx: &App) -> HttpFold {
        HttpFold {
            username: self.http_username.read(cx).value().to_string(),
            password: self.http_password.read(cx).value().to_string(),
            path: self.http_path.read(cx).value().to_string(),
            headers: self.http_headers.read(cx).value().to_string(),
        }
    }

    pub fn anytls_fold(&self, cx: &App) -> AnytlsFold {
        AnytlsFold {
            password: self.anytls_password.read(cx).value().to_string(),
            interval: self.anytls_interval.read(cx).value().to_string(),
            timeout: self.anytls_timeout.read(cx).value().to_string(),
            min: self.anytls_min.read(cx).value().to_string(),
        }
    }

    pub fn naive_fold(&self, cx: &App) -> NaiveFold {
        NaiveFold {
            username: self.naive_username.read(cx).value().to_string(),
            password: self.naive_password.read(cx).value().to_string(),
            udp_over_tcp: *self.naive_uot.borrow(),
            quic: *self.naive_quic.borrow(),
            congestion_control: Self::panel_value(&self.naive_cc, cx, ""),
        }
    }

    pub fn ssh_fold(&self, cx: &App) -> SshFold {
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

    pub fn snell_fold(&self, cx: &App) -> SnellFold {
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

    pub fn shadowtls_fold(&self, cx: &App) -> ShadowTlsFold {
        ShadowTlsFold {
            version: Self::panel_value(&self.shadowtls_version, cx, ""),
            password: self.shadowtls_password.read(cx).value().to_string(),
        }
    }

    pub fn xray_fold(&self, cx: &App) -> XrayFold {
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

    pub fn extra_fold(&self, cx: &App) -> ExtraFold {
        ExtraFold {
            exe: self.extra_exe.read(cx).value().to_string(),
            args: self.extra_args.read(cx).value().to_string(),
            config: self.extra_config.read(cx).value().to_string(),
        }
    }

    pub fn tls_forced_fold(&self, cx: &App) -> TlsForcedFold {
        TlsForcedFold {
            sni: self.s2_sni.read(cx).value().to_string(),
            alpn: self.s2_alpn.read(cx).value().to_string(),
            fp: Self::panel_value(&self.s2_fp, cx, ""),
            insecure: *self.s2_insecure.borrow(),
            pbk: self.s2_pbk.read(cx).value().to_string(),
            sid: self.s2_sid.read(cx).value().to_string(),
        }
    }

    pub fn transport_fold(&self, cx: &App) -> TransportFold {
        TransportFold {
            network: Self::panel_value(&self.s1_network, cx, "tcp"),
            path: self.s1_path.read(cx).value().to_string(),
            host: self.s1_host.read(cx).value().to_string(),
            method: self.s1_method.read(cx).value().to_string(),
            service_name: self.s1_service_name.read(cx).value().to_string(),
        }
    }

    pub fn tls_optional_fold(&self, cx: &App) -> TlsOptionalFold {
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

    pub fn s3_effective_mode(&self) -> String {
        if *self.s3_brutal.borrow() {
            "on".to_string()
        } else {
            self.s3_mode.borrow().clone()
        }
    }

    pub fn mux_fold(&self, cx: &App) -> MuxFold {
        MuxFold {
            mode: self.s3_effective_mode(),
            brutal_enable: *self.s3_brutal.borrow(),
            brutal_up_mbps: self.s3_up.read(cx).value().to_string(),
            brutal_down_mbps: self.s3_down.read(cx).value().to_string(),
        }
    }

    pub fn dial_fold(&self, cx: &App) -> DialFold {
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

    pub fn as_patch_vs(&self, cx: &App, baseline: &ProxyConfig) -> ConfigPatch {
        fn changed(current: String, baseline: &str) -> Option<String> {
            if current == baseline {
                None
            } else {
                Some(current)
            }
        }
        fn changed_secret(current: Secret, baseline: &Secret) -> Option<Secret> {
            if current.expose() == baseline.expose() {
                None
            } else {
                Some(current)
            }
        }
        let current = self.as_new(cx);
        ConfigPatch {
            name: changed(current.name, &baseline.name),
            protocol: changed(current.protocol, &baseline.protocol),
            params: changed_secret(current.params, &baseline.params),
            group: changed(current.group, &baseline.group),
            bind_ip: changed(current.bind_ip, &baseline.bind_ip),
            port: changed(current.port, &baseline.port.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{dialog_protocol_choices, protocol_choices};

    /// The dialog's protocol list is derived from the same table the picker
    /// reads, so a protocol added there can never go missing from the dialog.
    #[test]
    fn dialog_protocol_choices_mirror_the_picker_table() {
        let choices = dialog_protocol_choices();
        let source = protocol_choices();
        assert!(!choices.is_empty(), "the dialog offers protocols");
        assert_eq!(choices.len(), source.len(), "same table, same length");
        for (choice, expected) in choices.iter().zip(source.iter()) {
            assert_eq!(choice.value, expected.value, "order follows the table");
            assert_eq!(choice.title, expected.value, "title is the protocol");
            assert_eq!(
                choice.id,
                format!("protocol-option-{}", expected.value),
                "ids stay stable for the tests that click them"
            );
        }
    }
}
