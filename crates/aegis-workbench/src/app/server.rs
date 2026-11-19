//! The start screen's "Company server" column: renders `SignIn` and forwards
//! clicks and Enter presses; all sign-in rules live in `aegis_client::signin`.

use std::rc::Rc;
use std::sync::Arc;

use aegis_client::commands::Command;
use aegis_client::i18n::Catalog;
use aegis_client::session::Session;
use aegis_client::signin::{LoginForm, SignIn, SignInView, Stage, TRANSPORTS};
use aegis_model::{Methodology, Project};
use gpui::{
    App, AppContext as _, Context, Div, Entity, EventEmitter, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme as _, Disableable as _, Sizable as _};

use super::start::{ServerSetup, StartConfig};

/// The built-in methodology keys offered when creating a server project.
pub const METHODOLOGIES: [&str; 3] = ["iso21434-r155", "eu-cra", "stride-services-cloud"];

/// What the server pane asks the shell to show.
#[derive(Clone)]
pub enum ServerEvent {
    /// Show a server project's downloaded version read-only in the workbench.
    ShowProject {
        /// The downloaded project snapshot.
        project: Arc<Project>,
        /// The project's display name.
        name: String,
        /// Why the workbench is read-only.
        reason: String,
    },
}

/// The company-server column of the start screen.
pub struct ServerPane {
    catalog: Rc<Catalog>,
    signin: SignIn,
    address: Entity<InputState>,
    user: Entity<InputState>,
    password: Entity<InputState>,
    dev_user: Entity<InputState>,
    new_name: Entity<InputState>,
    transport: usize,
    remember: bool,
    methodology: usize,
    methodology_labels: [String; 3],
    company_offered: bool,
    shown: Option<(String, u64)>,
    closing: Option<Arc<Project>>,
    _subscriptions: [Subscription; 4],
}

impl EventEmitter<ServerEvent> for ServerPane {}

impl ServerPane {
    /// Builds the pane from remembered settings and calls `SignIn::start`.
    pub fn new(
        config: &StartConfig,
        setup: &ServerSetup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut signin = SignIn::new(
            setup.runtime.clone(),
            config.catalog.clone(),
            config.settings_path.clone(),
            Arc::clone(&setup.secrets),
        );
        let init = signin.view();
        signin.start();
        let mut input = |id, val: String, masked, cx: &mut Context<Self>| {
            let ph = config.catalog.tr(id);
            cx.new(|cx| {
                let s = InputState::new(window, cx)
                    .placeholder(ph)
                    .default_value(val);
                if masked { s.masked(true) } else { s }
            })
        };
        let address = input("server-address", init.server_url, false, cx);
        let user = input("server-user", init.user_name.clone(), false, cx);
        let password = input("server-password", String::new(), true, cx);
        let dev_user = input("server-dev-user", init.user_name, false, cx);
        let new_name = input("server-new-name", String::new(), false, cx);
        let transport = TRANSPORTS
            .iter()
            .position(|t| *t == init.transport)
            .unwrap_or(0);
        let subs = [
            on_enter(&address, window, cx, |s, _, cx| s.connect(cx)),
            on_enter(&password, window, cx, Self::log_in),
            on_enter(&dev_user, window, cx, |s, _, cx| s.dev_log_in(cx)),
            on_enter(&new_name, window, cx, Self::create_project),
        ];
        Self {
            catalog: config.catalog.clone(),
            signin,
            address,
            user,
            password,
            dev_user,
            new_name,
            transport,
            remember: false,
            methodology: 0,
            methodology_labels: methodology_labels(),
            company_offered: false,
            shown: None,
            closing: None,
            _subscriptions: subs,
        }
    }

    /// What the sign-in screen shows now.
    #[must_use]
    pub fn view(&self) -> SignInView {
        self.signin.view()
    }

    /// The running session, once one is started.
    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        self.signin.session()
    }

    /// Connects to the address in the input with the chosen transport.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        let url = self.address.read(cx).value().to_string();
        let transport = TRANSPORTS.get(self.transport).copied().unwrap_or("auto");
        self.company_offered = false;
        self.signin.connect(&url, transport);
        cx.notify();
    }

    /// Logs in with user name and password, then clears the password input.
    pub fn log_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let user = self.user.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        self.signin.password_log_in(&user, &password, self.remember);
        set_input(&self.password, "", window, cx);
        cx.notify();
    }

    /// Starts the browser-based company login.
    pub fn company_log_in(&mut self, cx: &mut Context<Self>) {
        self.signin.company_log_in();
        cx.notify();
    }

    /// Logs in with the development user name.
    pub fn dev_log_in(&mut self, cx: &mut Context<Self>) {
        let user = self.dev_user.read(cx).value().to_string();
        self.signin.dev_log_in(&user);
        cx.notify();
    }

    /// Switches from company login to the user name and password form.
    pub fn use_password(&mut self, cx: &mut Context<Self>) {
        self.company_offered = matches!(self.signin.view().form, Some(LoginForm::Company { .. }));
        self.signin.use_password();
        cx.notify();
    }

    /// Switches back to company login.
    pub fn use_company(&mut self, cx: &mut Context<Self>) {
        self.company_offered = false;
        self.signin.use_company();
        cx.notify();
    }

    /// Leaves the current server and returns to the address form.
    pub fn change_server(&mut self, cx: &mut Context<Self>) {
        (self.company_offered, self.shown, self.closing) = (false, None, None);
        self.signin.change_server();
        cx.notify();
    }

    /// Signs out of the current server.
    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        (self.shown, self.closing) = (None, None);
        self.signin.sign_out();
        cx.notify();
    }

    /// Reloads the server's project list.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.send(Command::Refresh);
        cx.notify();
    }

    /// Opens the server project with `id`.
    pub fn open_project(&mut self, id: &str, cx: &mut Context<Self>) {
        self.send(Command::OpenProject { id: id.to_owned() });
        cx.notify();
    }

    /// Creates a project from the trimmed name input and chosen methodology.
    pub fn create_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let key = METHODOLOGIES
            .get(self.methodology)
            .copied()
            .unwrap_or(METHODOLOGIES[0]);
        self.send(Command::CreateProject {
            name,
            methodology_key: key.to_owned(),
        });
        set_input(&self.new_name, "", window, cx);
        cx.notify();
    }

    /// Closes the open server project and clears what was shown.
    pub fn close_project(&mut self, cx: &mut Context<Self>) {
        self.closing = self
            .signin
            .session()
            .and_then(|s| s.view().project.as_ref().map(|p| Arc::clone(&p.snapshot)));
        self.send(Command::CloseProject);
        self.shown = None;
        cx.notify();
    }

    /// Moves the keyboard focus to the server address field.
    pub fn focus_address(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.address.update(cx, |a, cx| a.focus(window, cx));
    }

    /// Puts `text` in the server address field.
    pub fn set_address(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_input(&self.address, text, window, cx);
    }

    /// Puts `text` in the user name field.
    pub fn set_user(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_input(&self.user, text, window, cx);
    }

    /// Puts `text` in the password field.
    pub fn set_password(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_input(&self.password, text, window, cx);
    }

    /// Puts `text` in the new project name field.
    pub fn set_new_name(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        set_input(&self.new_name, text, window, cx);
    }

    /// Drains sign-in and session updates, emitting [`ServerEvent::ShowProject`]
    /// when a new downloaded project version arrives.
    pub fn pump(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = self.signin.pump();
        if self.company_offered && self.signin.view().form != Some(LoginForm::Password) {
            self.company_offered = false;
            changed = true;
        }
        let open = self.signin.session().and_then(|s| {
            let model = s.view();
            let p = model.project.as_ref()?;
            let key = (p.entry.id.clone(), p.snapshot_seq);
            Some((
                key,
                p.entry.name.clone(),
                p.lock_holder.clone(),
                Arc::clone(&p.snapshot),
            ))
        });
        match open {
            None => (self.closing, self.shown) = (None, None),
            Some((_, _, _, s)) if self.closing.as_ref().is_some_and(|c| Arc::ptr_eq(c, &s)) => {}
            Some((key, name, holder, project)) => {
                self.closing = None;
                if self.shown.as_ref() != Some(&key) {
                    let reason = self.read_only_reason(holder.as_deref());
                    self.shown = Some(key);
                    changed = true;
                    cx.emit(ServerEvent::ShowProject {
                        project,
                        name,
                        reason,
                    });
                }
            }
        }
        if changed {
            cx.notify();
        }
        changed
    }

    fn read_only_reason(&self, holder: Option<&str>) -> String {
        let v = self.signin.view();
        let server = if v.server_name.is_empty() {
            v.server_url
        } else {
            v.server_name
        };
        match holder {
            Some(h) => self.catalog.tr_with(
                "workbench-read-only-server-held",
                &[("server", &server), ("holder", h)],
            ),
            None => self
                .catalog
                .tr_with("workbench-read-only-server", &[("server", &server)]),
        }
    }

    fn send(&self, command: Command) {
        if let Some(session) = self.signin.session() {
            let _ = session.send(command);
        }
    }

    fn tr(&self, id: &str) -> SharedString {
        SharedString::from(self.catalog.tr(id))
    }

    fn btn(
        &self,
        id: &'static str,
        cx: &mut Context<Self>,
        on: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> Button {
        Button::new(id)
            .label(self.tr(id))
            .on_click(cx.listener(move |s, _, w, cx| on(s, w, cx)))
    }

    fn render_choose(&self, view: &SignInView, cx: &mut Context<Self>) -> Div {
        let transports = choice(
            "server-transport",
            &TRANSPORTS,
            self.transport,
            cx,
            |s, i| {
                s.transport = i;
            },
        );
        let connect = self.btn("server-connect", cx, |s, _, cx| s.connect(cx));
        col()
            .child(field(self.tr("server-address"), Input::new(&self.address)))
            .child(field(self.tr("server-transport"), transports))
            .child(row(connect.primary().disabled(view.busy)))
    }

    fn render_login(&self, view: &SignInView, cx: &mut Context<Self>) -> Div {
        match view.form {
            Some(LoginForm::Company { password_also }) => {
                let company = self
                    .btn("server-company-log-in", cx, |s, _, cx| s.company_log_in(cx))
                    .primary()
                    .disabled(view.busy);
                let alt = password_also.then(|| {
                    row(self
                        .btn("server-use-password", cx, |s, _, cx| s.use_password(cx))
                        .ghost())
                });
                col().child(row(company)).children(alt)
            }
            Some(LoginForm::Password) => {
                let remember = Checkbox::new("server-remember")
                    .label(self.tr("server-remember"))
                    .checked(self.remember)
                    .on_click(cx.listener(|s, on: &bool, _, cx| {
                        s.remember = *on;
                        cx.notify();
                    }));
                let login = self
                    .btn("server-log-in", cx, Self::log_in)
                    .primary()
                    .disabled(view.busy);
                let alt = self.company_offered.then(|| {
                    self.btn("server-use-company", cx, |s, _, cx| s.use_company(cx))
                        .ghost()
                });
                col()
                    .child(field(self.tr("server-user"), Input::new(&self.user)))
                    .child(field(
                        self.tr("server-password"),
                        Input::new(&self.password).mask_toggle(),
                    ))
                    .child(remember)
                    .child(row(login).children(alt))
            }
            Some(LoginForm::Development) => {
                let login = Button::new("server-dev-log-in")
                    .primary()
                    .label(self.tr("server-log-in"))
                    .disabled(view.busy)
                    .on_click(cx.listener(|s, _, _, cx| s.dev_log_in(cx)));
                col()
                    .child(field(
                        self.tr("server-dev-user"),
                        Input::new(&self.dev_user),
                    ))
                    .child(row(login))
            }
            None => col(),
        }
    }

    fn render_online(&self, view: &SignInView, cx: &mut Context<Self>) -> Div {
        let open_label = self.tr("server-open");
        let list = if view.projects.is_empty() {
            muted(cx, self.tr("server-no-projects"))
        } else {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(view.projects.iter().enumerate().map(|(ix, p)| {
                    let id = p.id.clone();
                    let open = Button::new(("server-open", ix))
                        .small()
                        .label(open_label.clone())
                        .on_click(cx.listener(move |s, _, _, cx| s.open_project(&id, cx)));
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().truncate().child(p.name.clone()))
                        .child(open)
                }))
        };
        let methods = choice(
            "server-methodology",
            &self.methodology_labels,
            self.methodology,
            cx,
            |s, i| {
                s.methodology = i;
            },
        );
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(self.tr("server-projects")))
            .child(
                self.btn("server-refresh", cx, |s, _, cx| s.refresh(cx))
                    .small(),
            )
            .child(
                self.btn("server-sign-out", cx, |s, _, cx| s.sign_out(cx))
                    .small()
                    .ghost(),
            );
        let create = self
            .btn("server-create", cx, Self::create_project)
            .primary();
        col()
            .child(header)
            .child(list)
            .child(field(
                self.tr("server-new-name"),
                Input::new(&self.new_name),
            ))
            .child(methods)
            .child(row(create))
    }
}

impl Render for ServerPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.signin.view();
        let body = match view.stage {
            Stage::ChooseServer => Some(self.render_choose(&view, cx)),
            Stage::LoginRequired => Some(self.render_login(&view, cx)),
            Stage::Online => Some(self.render_online(&view, cx)),
            Stage::Connecting | Stage::Offline => None,
        };
        let show_change = view.stage != Stage::ChooseServer;
        let name =
            (show_change && !view.server_name.is_empty()).then(|| muted(cx, view.server_name));
        let change = show_change.then(|| {
            row(self
                .btn("server-change", cx, |s, _, cx| s.change_server(cx))
                .ghost())
        });
        let danger = cx.theme().danger;
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_lg().child(self.tr("server-title")))
            .children(name)
            .child(muted(cx, view.status))
            .children(body)
            .children(view.error.map(|e| div().text_color(danger).child(e)))
            .children(change)
    }
}

fn col() -> Div {
    div().flex().flex_col().gap_3()
}

fn row(el: impl IntoElement) -> Div {
    div().flex().gap_2().child(el)
}

fn muted(cx: &App, text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn field(label: SharedString, input: impl IntoElement) -> Div {
    div().flex().flex_col().gap_1().child(label).child(input)
}

fn choice(
    prefix: &'static str,
    labels: &[impl AsRef<str>],
    active: usize,
    cx: &mut Context<ServerPane>,
    pick: fn(&mut ServerPane, usize),
) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap_1()
        .children(labels.iter().enumerate().map(|(ix, label)| {
            let btn = Button::new((prefix, ix))
                .small()
                .label(label.as_ref().to_owned())
                .on_click(cx.listener(move |s, _, _, cx| {
                    pick(s, ix);
                    cx.notify();
                }));
            if active == ix { btn.primary() } else { btn }
        }))
}

fn set_input(input: &Entity<InputState>, text: &str, window: &mut Window, cx: &mut App) {
    let text = text.to_owned();
    input.update(cx, |i, cx| i.set_value(text, window, cx));
}

fn on_enter(
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<ServerPane>,
    action: fn(&mut ServerPane, &mut Window, &mut Context<ServerPane>),
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                action(this, window, cx);
            }
        },
    )
}

fn methodology_labels() -> [String; 3] {
    [
        Methodology::iso21434_r155(),
        Methodology::eu_cra(),
        Methodology::stride_services_cloud(),
    ]
    .map(|r| r.map(|m| m.label().to_owned()).unwrap_or_default())
}
