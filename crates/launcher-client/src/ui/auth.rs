//! Before the launcher proper: which server, and who is playing.

use gpui_kit::component::{ActiveTheme, StyledExt};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, SharedString, Window, div, img, px};
use relic_proto::RegistrationPolicy;

use super::app::{Launcher, Screen};
use super::icon::{AnimatedIcon, IconName};
use crate::lang::{Msg, t};

use super::widgets::{Btn, Field, board, message, pixel, tile};

impl Launcher {
    /// Error or notice under a form, whichever there is.
    pub(super) fn feedback(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if let Some(error) = &self.error {
            Some(message("error", error.clone(), true, cx).into_any_element())
        } else {
            self.notice
                .as_ref()
                .map(|notice| message("notice", notice.clone(), false, cx).into_any_element())
        }
    }

    /// A mark, a title and a line under it, with room for fields below.
    fn form(
        &self,
        mark: AnyElement,
        title: SharedString,
        subtitle: SharedString,
        cx: &Context<Self>,
    ) -> gpui_kit::Div {
        let dim = cx.theme().colors.muted_foreground;
        div().v_flex().w_full().gap_4().child(mark).child(
            div()
                .v_flex()
                .gap_1()
                .pb_2()
                .child(pixel(title, 16.))
                .child(div().text_sm().text_color(dim).child(subtitle)),
        )
    }

    /// The form in a tile on the left, the scene filling the rest.
    fn beside_scene(
        &self,
        form: gpui_kit::Div,
        links: Option<gpui_kit::Div>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let surface = cx.theme().colors.secondary;
        board(cx)
            .h_flex()
            .size_full()
            .child(
                div()
                    .v_flex()
                    .flex_none()
                    .w(px(420.))
                    .h_full()
                    .justify_center()
                    .gap_3()
                    .px_10()
                    .bg(surface)
                    .child(form)
                    .children(links),
            )
            .child(
                tile(cx).relative().flex_1().min_w_0().h_full().child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .child(self.scene()),
                ),
            )
            .into_any_element()
    }

    pub(super) fn connect_screen(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let accent = cx.theme().colors.primary;
        let mark = AnimatedIcon::new("connect-mark", IconName::Server, accent)
            .size(px(44.))
            .looping(self.busy)
            .into_any_element();
        let form = self
            .form(mark, "Relic".into(), t(Msg::ConnectHint).into(), cx)
            .child(
                Field::new(
                    "address",
                    t(Msg::ServerAddress),
                    IconName::Globe,
                    &self.address,
                )
                .disabled(self.busy),
            )
            .children(self.feedback(cx))
            .child(
                Btn::new("connect")
                    .label(t(Msg::Connect))
                    .icon(IconName::ChevronRight)
                    .primary()
                    .full()
                    .loading(self.busy)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.connect(window, cx);
                    })),
            );
        self.beside_scene(form, None, cx)
    }

    pub(super) fn auth_screen(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let accent = cx.theme().colors.primary;
        let screen = self.screen;
        let policy = self.registration();
        let server_name: SharedString = self
            .handshake
            .as_ref()
            .map(|h| h.server_name.clone())
            .unwrap_or_default()
            .into();
        let mark = match &self.logo {
            Some(logo) => img(logo.clone())
                .h(px(64.))
                .max_w(px(240.))
                .into_any_element(),
            None => AnimatedIcon::new("auth-mark", IconName::Server, accent)
                .size(px(44.))
                .play(1)
                .into_any_element(),
        };
        let subtitle = match screen {
            Screen::Register if policy == RegistrationPolicy::ApprovalRequired => {
                t(Msg::RegisterApprovalHint)
            }
            Screen::Register => t(Msg::RegisterInviteHint),
            Screen::Reset => t(Msg::ResetHint),
            _ => t(Msg::SignInHint),
        };

        let mut form = self.form(mark, server_name, subtitle.into(), cx).child(
            Field::new("username", t(Msg::Username), IconName::User, &self.username)
                .disabled(self.busy),
        );
        if screen == Screen::Reset {
            form = form.child(
                Field::new("code", t(Msg::ResetCode), IconName::Key, &self.code)
                    .disabled(self.busy),
            );
        }
        form = form.child(
            Field::new(
                "password",
                if screen == Screen::Reset {
                    t(Msg::NewPassword)
                } else {
                    t(Msg::Password)
                },
                IconName::Lock,
                &self.password,
            )
            .disabled(self.busy),
        );
        if screen == Screen::Register && policy == RegistrationPolicy::StrictInvite {
            form = form.child(
                Field::new("invite", t(Msg::InviteCode), IconName::Ticket, &self.invite)
                    .disabled(self.busy),
            );
        }
        form = form.children(self.feedback(cx));

        let submit = Btn::new("submit").primary().full().loading(self.busy);
        form = form.child(match screen {
            Screen::Register => submit
                .label(t(Msg::CreateAccount))
                .icon(IconName::Plus)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.register(window, cx);
                })),
            Screen::Reset => submit
                .label(t(Msg::ChangePassword))
                .icon(IconName::Key)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.reset_password(window, cx);
                })),
            _ => submit
                .label(t(Msg::SignIn))
                .icon(IconName::ChevronRight)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.login(window, cx);
                })),
        });

        let to = |id: &'static str, label: &'static str, icon, target: Screen| {
            Btn::new(id)
                .label(label)
                .icon(icon)
                .quiet()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(target, cx)))
        };
        // Frameless buttons: pulled left so their icons line up with the fields.
        let mut links = div().h_flex().flex_wrap().ml(px(-16.));
        if screen != Screen::Login {
            links = links.child(to(
                "to-login",
                t(Msg::SignIn),
                IconName::ArrowLeft,
                Screen::Login,
            ));
        }
        if screen != Screen::Register && policy != RegistrationPolicy::Closed {
            links = links.child(to(
                "to-register",
                t(Msg::CreateAccount),
                IconName::Plus,
                Screen::Register,
            ));
        }
        if screen != Screen::Reset {
            links = links.child(to(
                "to-reset",
                t(Msg::ResetPassword),
                IconName::Key,
                Screen::Reset,
            ));
        }
        links = links.child(
            Btn::new("to-connect")
                .label(t(Msg::OtherServer))
                .icon(IconName::Globe)
                .quiet()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.change_server(cx))),
        );

        self.beside_scene(form, Some(links), cx)
    }
}
