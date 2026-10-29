use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use gpui::PathPromptOptions;
use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, img};
use i18n::t;
use state::{Outcome, Toast, Toasts};
use ui::{ActiveTheme as _, Avatar, Button, Switch, Text};

use super::rows::outward;
use super::{Group, SettingsView, Slot, group_heading};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How large the app icon stands, in covers.
const MARK_TILE: f32 = 0.5;
/// The app icon as this platform shows it, registered by the binary's asset source.
const APP_ICON: &str = "brand/sonora.svg";
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
const SOURCE_URL: &str = "https://github.com/DeeJayKyle/DIGIDA";

#[derive(Clone, Copy)]
struct Member {
    login: &'static str,
    avatar: &'static str,
    profile: &'static str,
    role: Role,
}

#[derive(Clone, Copy)]
enum Role {
    LeadMaintainer,
    Maintainer,
    Contributor,
}

impl Role {
    fn label(self) -> SharedString {
        match self {
            Self::LeadMaintainer => t!("settings-role-lead-maintainer"),
            Self::Maintainer => t!("settings-role-maintainer"),
            Self::Contributor => t!("settings-role-contributor"),
        }
    }
}

macro_rules! member {
    ($login:literal, $role:expr) => {
        Member {
            login: $login,
            avatar: concat!("https://github.com/", $login, ".png"),
            profile: concat!("https://github.com/", $login),
            role: $role,
        }
    };
}

/// What a settings backup is called when it is first saved.
const BACKUP_NAME: &str = "digida-settings.json";

const MEMBERS: [Member; 5] = [
    member!("nolight132", Role::LeadMaintainer),
    member!("zxsleebu", Role::Maintainer),
    member!("fx-got", Role::Maintainer),
    member!("Makakashan", Role::Contributor),
    member!("imizgun", Role::Contributor),
];

impl SettingsView {
    pub(super) fn about_groups(&self) -> Vec<Group> {
        vec![Group::untitled().rows([
            Slot::Version,
            Slot::Updates,
            Slot::Log,
            Slot::Backup,
            Slot::License,
            Slot::Source,
        ])]
    }

    pub(super) fn version_row(&self) -> AnyElement {
        self.row(
            t!("settings-version"),
            div().child(VERSION).into_any_element(),
        )
    }

    pub(super) fn updates_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).check_updates();

        self.row(
            t!("settings-check-updates"),
            Switch::new("check-updates", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_check_updates(!on, cx));
                }))
                .into_any_element(),
        )
    }

    /// The row that opens a copy of the current log file in whatever the system reads text
    /// with. It is disabled when the platform names no state or cache folder to log into.
    pub(super) fn log_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = state::log_file();

        outward("open-log", t!("settings-log-open"), None, cx)
            .disabled(path.is_none())
            .on_click(move |_, _, _| {
                let Some(path) = &path else { return };
                if let Err(error) = snapshot(path).and_then(open::that_detached) {
                    log::warn!(
                        "settings: cannot open a copy of {}: {error}",
                        path.display()
                    );
                }
            })
            .into_any_element()
    }

    /// The row that saves the settings to a file and puts a saved file back in their place.
    pub(super) fn backup_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let button =
            |id: &'static str, label: SharedString| Button::new(id).small().outline().label(label);
        self.row(
            t!("settings-backup"),
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    button("settings-backup-save", t!("settings-backup-save"))
                        .on_click(cx.listener(|this, _, _, cx| this.save_backup(cx))),
                )
                .child(
                    button("settings-backup-restore", t!("settings-backup-restore"))
                        .on_click(cx.listener(|this, _, _, cx| this.restore_backup(cx))),
                )
                .into_any_element(),
        )
    }

    /// Asks where to write the settings and writes them there.
    fn save_backup(&mut self, cx: &mut Context<Self>) {
        let bytes = match self.settings.read(cx).backup() {
            Ok(bytes) => bytes,
            Err(error) => {
                log::warn!("settings: cannot back up: {error:#}");
                Toasts::show(Outcome::Failed, "settings-backup-failed", cx);
                return;
            }
        };
        let directory = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_default();
        let receiver = cx.prompt_for_new_path(&directory, Some(BACKUP_NAME));
        self.backup = Some(cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let written = cx
                .background_spawn(async move { std::fs::write(path, bytes) })
                .await;
            cx.update(|cx| match written {
                Ok(()) => Toasts::show(Outcome::Done, "settings-backup-saved", cx),
                Err(error) => {
                    log::warn!("settings: cannot write the backup: {error}");
                    Toasts::post(
                        Toast::new(Outcome::Failed, "settings-backup-failed")
                            .detail(error.to_string()),
                        cx,
                    );
                }
            });
        }));
    }

    /// Asks for a backup and puts it in place of the settings, which then reload.
    fn restore_backup(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        self.backup = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let read = cx
                .background_spawn(async move { std::fs::read(path) })
                .await;
            this.update(cx, |this, cx| {
                let restored = match read {
                    Ok(bytes) => this
                        .settings
                        .read(cx)
                        .restore(&bytes)
                        .map_err(|error| format!("{error:#}")),
                    Err(error) => Err(error.to_string()),
                };
                match restored {
                    Ok(()) => Toasts::show(Outcome::Done, "settings-backup-restored", cx),
                    Err(error) => {
                        log::warn!("settings: cannot restore the backup: {error}");
                        Toasts::post(
                            Toast::new(Outcome::Failed, "settings-backup-unreadable").detail(error),
                            cx,
                        );
                    }
                }
            })
            .ok();
        }));
    }

    pub(super) fn license_row(&self, cx: &mut Context<Self>) -> AnyElement {
        outward(
            "license",
            t!("settings-license"),
            Some(t!("settings-license-short")),
            cx,
        )
        .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
        .into_any_element()
    }

    pub(super) fn source_row(&self, cx: &mut Context<Self>) -> AnyElement {
        outward("source", t!("settings-source"), None, cx)
            .on_click(|_, _, cx| cx.open_url(SOURCE_URL))
            .into_any_element()
    }

    /// Opens About with the app icon beside its name, aligned with the settings below. The icon
    /// is the one this platform shows, in its own colours whatever the theme.
    pub(super) fn mark(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let tile = theme.metrics.cover * MARK_TILE;

        div()
            .flex()
            .items_center()
            .gap_5()
            .child(img(APP_ICON).size(tile).flex_none())
            .child(
                div()
                    .text_size(theme.text(Text::Title))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t!("settings-about-name")),
            )
    }

    pub(super) fn notice(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .text_color(theme.muted_foreground)
            .text_size(theme.text(Text::Small))
            .child(t!("settings-notice"))
    }

    /// The people behind Sonora, each opening their GitHub profile.
    pub(super) fn team(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .flex()
            .flex_col()
            .child(group_heading(
                t!("settings-team"),
                Some("icons/user-round.svg"),
                cx,
            ))
            .children(MEMBERS.into_iter().enumerate().map(|(index, member)| {
                div()
                    .id(("team-member", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(theme.metrics.list_row)
                    .mx_neg_3()
                    .px_3()
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.secondary_hover))
                    .on_click(move |_, _, cx| cx.open_url(member.profile))
                    .child(Avatar::new(Some(member.avatar)).size(theme.metrics.thumb))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(member.login),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme.text(Text::Label))
                            .text_color(theme.muted_foreground)
                            .child(member.role.label()),
                    )
            }))
    }
}

/// Notepad on Windows refuses a file that Sonora still writes to, so the button opens a copy.
/// `File::open` shares write access, and `fs::copy` documents no share mode.
fn snapshot(log: &Path) -> io::Result<PathBuf> {
    let copy = log.with_file_name("digida.copy.log");
    io::copy(&mut File::open(log)?, &mut File::create(&copy)?)?;
    Ok(copy)
}
