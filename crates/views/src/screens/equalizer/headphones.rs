use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, Window, div};
use i18n::t;
use ui::Button;

use super::{EqualizerView, heading};
use crate::screens::settings::Row;

/// The icon before the group's heading.
const ICON: &str = "icons/headphones.svg";

impl EqualizerView {
    /// The headphone group: a field that searches AutoEQ's measurements, and a row per match
    /// whose button plays that headphone's correction as a new profile.
    pub(super) fn headphones_group(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let headphones = self.headphones.read(cx);
        let searched = !self.headphone_query.read(cx).text().trim().is_empty();
        let note: Option<SharedString> = match () {
            _ if headphones.loading() => Some(t!("equalizer-headphones-loading")),
            _ if headphones.unreachable() => Some(t!("equalizer-headphones-unreachable")),
            _ if searched && headphones.found().is_empty() => Some(t!("equalizer-headphones-none")),
            _ => None,
        };
        let found: Vec<AnyElement> = headphones
            .found()
            .iter()
            .enumerate()
            .map(|(at, headphone)| {
                let chosen = headphone.clone();
                let apply = Button::new(("equalizer-headphone", at))
                    .small()
                    .outline()
                    .label(t!("equalizer-headphone-apply"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let chosen = chosen.clone();
                        this.headphones
                            .update(cx, |headphones, cx| headphones.apply(chosen, cx));
                    }))
                    .into_any_element();
                let row = Row::new(headphone.name.clone().into(), apply);
                match headphone.source.is_empty() {
                    true => row,
                    false => row.detail(t!(
                        "equalizer-headphone-source",
                        source = headphone.source.clone()
                    )),
                }
                .into_any_element()
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .child(heading("equalizer-headphones", ICON, None, cx))
            .child(
                Row::new(
                    t!("equalizer-headphones-search"),
                    div()
                        .w_full()
                        .child(self.headphone_query.clone())
                        .into_any_element(),
                )
                .detail(t!("equalizer-headphones-detail")),
            )
            .children(note.map(|note| div().child(note)))
            .children(found)
            .into_any_element()
    }
}
