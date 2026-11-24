//! The Shortcuts settings tab: every bindable action with its current key
//! combo(s). Clicking a binding starts a capture — the next key press
//! becomes the new binding — and Reset restores the default. Changes apply
//! immediately (the global key handler routes through the same map) and
//! persist in ui.json.

use iced::widget::{button, column, row, text};
use iced::{Element, Theme};

use crate::message::Message;
use crate::shortcuts::ShortcutAction;
use crate::theme as th;

use super::views_settings_rows::{action, group, hint, setting_row};
use super::*;

fn semibold() -> iced::Font {
    iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }
}

/// The binding button: shows the combo(s), or "Press keys…" while capturing.
fn combo_button(label: String, capturing: bool, msg: Message) -> Element<'static, Message> {
    button(
        text(label)
            .size(12)
            .color(if capturing { th::accent() } else { th::text() }),
    )
    .on_press(msg)
    .padding([4, 10])
    .style(move |_theme: &Theme, status| {
        let mut style = crate::widgets::control_style::styled(None, status);
        if capturing {
            style.border = iced::Border {
                color: th::accent(),
                width: 1.5,
                radius: style.border.radius,
            };
        }
        style
    })
    .into()
}

impl App {
    pub(super) fn view_settings_shortcuts_tab(&self) -> Element<'_, Message> {
        let on_macos = cfg!(target_os = "macos");
        let capturing = self.state.view.shortcut_capture;

        let mut body = column![hint(
            "Click a binding, then press the new key combination — or turn the mouse wheel for a wheel binding. Esc cancels. Changes apply at once and are saved for you, not in the project."
        )]
        .spacing(16);

        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        let mut current_group: Option<&'static str> = None;
        for &shortcut_action in ShortcutAction::ALL {
            if current_group != Some(shortcut_action.group()) {
                if !rows.is_empty() {
                    body = body.push(group(std::mem::take(&mut rows)));
                }
                current_group = Some(shortcut_action.group());
                body = body.push(
                    text(shortcut_action.group())
                        .size(13)
                        .font(semibold())
                        .color(th::text_dim()),
                );
            }
            let is_capturing = capturing == Some(shortcut_action);
            let combos = self.shortcuts.combos_for(shortcut_action);
            let label = if is_capturing {
                "Press keys…".to_string()
            } else if combos.is_empty() {
                "—".to_string()
            } else {
                combos
                    .iter()
                    .map(|combo| combo.label(on_macos))
                    .collect::<Vec<_>>()
                    .join("   /   ")
            };
            let is_custom = self.shortcuts.is_custom(shortcut_action);
            let control = row![
                combo_button(
                    label,
                    is_capturing,
                    Message::BeginShortcutCapture(shortcut_action),
                ),
                action(
                    "Reset",
                    if is_custom {
                        Some(Message::ResetShortcut(shortcut_action))
                    } else {
                        None
                    },
                    false,
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center);
            rows.push(setting_row(shortcut_action.label(), None, control));
        }
        if !rows.is_empty() {
            body = body.push(group(rows));
        }

        body.into()
    }
}
