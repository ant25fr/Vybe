//! The Ctrl+P quick-add palette overlay: a search field, the filtered item
//! list (arrow keys + Enter), and the kind filter row at its foot.

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input};
use iced::{Element, Length, Theme};

use crate::icons;
use crate::message::{Message, QuickAddItem};
use crate::state::QuickAddKind;
use crate::theme as th;

use super::*;

/// The palette's text field id, focused when the palette opens.
pub(super) fn quick_add_input_id() -> iced::widget::text_input::Id {
    iced::widget::text_input::Id::new("quick-add-input")
}

const PALETTE_WIDTH: f32 = 460.0;
const LIST_MAX_HEIGHT: f32 = 320.0;

fn palette_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(th::bg_surface().into()),
        border: iced::Border {
            color: th::border(),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn kind_chip(kind: QuickAddKind, active: bool) -> Element<'static, Message> {
    button(text(kind.label()).size(10).color(if active {
        th::accent()
    } else {
        th::text_muted()
    }))
    .on_press(Message::QuickAddSetKind(kind))
    .padding([4, 9])
    .style(move |_theme: &Theme, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => th::bg_hover(),
            _ => {
                if active {
                    th::bg_hover()
                } else {
                    iced::Color::TRANSPARENT
                }
            }
        };
        button::Style {
            background: Some(bg.into()),
            text_color: if active {
                th::accent()
            } else {
                th::text_muted()
            },
            border: iced::Border {
                color: if active { th::accent() } else { th::border() },
                width: 1.0,
                radius: 9.0.into(),
            },
            ..Default::default()
        }
    })
    .into()
}

impl App {
    pub(super) fn view_quick_add_overlay(&self) -> Element<'_, Message> {
        let Some(state) = &self.state.view.quick_add else {
            return iced::widget::Space::with_width(0).into();
        };
        let items = self.quick_add_items(state);
        let selected = state.selected.min(items.len().saturating_sub(1));

        let search = text_input("Quick add…", &state.query)
            .id(quick_add_input_id())
            .on_input(Message::QuickAddQueryChanged)
            .on_submit(Message::QuickAddSubmit)
            .icon(text_input::Icon {
                font: icons::ICON_FONT,
                code_point: icons::SEARCH,
                size: Some(13.0.into()),
                spacing: 7.0,
                side: text_input::Side::Left,
            })
            .size(13)
            .padding([8, 10])
            .width(Length::Fill);

        let mut list = column![].spacing(1);
        if items.is_empty() {
            list = list.push(
                container(text("Nothing matches").size(11).color(th::text_dim())).padding([8, 10]),
            );
        }
        for (index, item) in items.iter().enumerate() {
            list = list.push(self.view_quick_add_row(item, index == selected));
        }

        let mut kinds = row![].spacing(5);
        for kind in QuickAddKind::ALL {
            kinds = kinds.push(kind_chip(kind, state.kind == kind));
        }

        let card =
            container(
                column![
                    search,
                    container(scrollable(container(list).width(Length::Fill)).height(
                        Length::Fixed(LIST_MAX_HEIGHT.min(28.0 * items.len().max(1) as f32 + 8.0))
                    ))
                    .width(Length::Fill),
                    container(kinds)
                        .padding(iced::Padding {
                            top: 6.0,
                            right: 10.0,
                            bottom: 8.0,
                            left: 10.0,
                        })
                        .width(Length::Fill),
                ]
                .spacing(0),
            )
            .width(Length::Fixed(PALETTE_WIDTH))
            .style(palette_card_style);

        // Centered near the top of the window; the backdrop click closes.
        let positioned = column![
            iced::widget::vertical_space().height(Length::Fixed(60.0)),
            container(card).width(Length::Fill).center_x(PALETTE_WIDTH),
        ];
        mouse_area(
            container(positioned)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(Message::CloseQuickAdd)
        .into()
    }

    fn view_quick_add_row(&self, item: &QuickAddItem, selected: bool) -> Element<'_, Message> {
        let icon = match item {
            QuickAddItem::Instrument(_) => icons::MUSIC,
            QuickAddItem::Effect(_) => icons::SLIDERS_VERTICAL,
            QuickAddItem::Plugin(_) => icons::PLUG,
            QuickAddItem::Sample(_) => icons::AUDIO_WAVEFORM,
        };
        let msg = Message::QuickAddDevice(item.clone());
        button(
            row![
                icons::icon(icon).size(13).color(if selected {
                    th::accent()
                } else {
                    th::text_dim()
                }),
                text(item.label())
                    .size(12)
                    .color(if selected { th::text() } else { th::text_dim() })
                    .width(Length::Fill),
                text(item.kind_label()).size(9).color(th::text_muted()),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .on_press(msg)
        .padding([5, 10])
        .width(Length::Fill)
        .style(move |_theme: &Theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => th::bg_hover(),
                _ => {
                    if selected {
                        th::bg_hover()
                    } else {
                        iced::Color::TRANSPARENT
                    }
                }
            };
            button::Style {
                background: Some(bg.into()),
                text_color: th::text(),
                border: iced::Border::default(),
                ..Default::default()
            }
        })
        .into()
    }
}
