//! The browser's sample map: one coloured point per sample on a grid, kinds
//! detected from the file names, hover to play, click to select, double-click
//! to import, drag to the arrangement. A legend under the map filters kinds.

use iced::mouse;
use iced::widget::{button, canvas, column, container, row, text};
use iced::{Element, Length, Point, Rectangle, Theme, Vector};

use crate::domains::browser::BrowserMsg;
use crate::message::Message;
use crate::state::{BrowserResultsView, SampleMapKind, SAMPLE_MAP_KINDS};
use crate::theme as th;
use vibez_core::track::MediaSourceRef;

use super::*;

/// One point of the map.
#[derive(Debug, Clone)]
pub(super) struct SampleMapPoint {
    pub source: MediaSourceRef,
    pub name: String,
    pub kind: SampleMapKind,
}

const CELL: f32 = 26.0;
/// How far a press may move before it becomes a drag.
const DRAG_THRESHOLD: f32 = 6.0;
/// Double-click window.
const DOUBLE_CLICK_MS: u128 = 400;
/// Points are capped so a huge library still draws in one frame.
const MAX_POINTS: usize = 2000;

#[derive(Debug, Default)]
pub(super) struct SampleMapInteraction {
    hovered: Option<usize>,
    pressed: Option<(usize, Point)>,
    dragged: bool,
    last_click: Option<(usize, std::time::Instant)>,
}

pub(super) struct SampleMapCanvas {
    pub points: Vec<SampleMapPoint>,
    pub selected: Option<MediaSourceRef>,
    pub kind_filter: Option<SampleMapKind>,
}

impl SampleMapCanvas {
    fn columns(&self, width: f32) -> usize {
        ((width / CELL) as usize).max(1)
    }

    fn point_pos(&self, index: usize, width: f32) -> Point {
        let columns = self.columns(width);
        let col = index % columns;
        let row = index / columns;
        Point::new(
            col as f32 * CELL + CELL / 2.0,
            row as f32 * CELL + CELL / 2.0,
        )
    }

    fn hit(&self, pos: Point, width: f32) -> Option<usize> {
        if pos.x < 0.0 || pos.y < 0.0 {
            return None;
        }
        let columns = self.columns(width);
        let col = (pos.x / CELL) as usize;
        let row = (pos.y / CELL) as usize;
        let index = row * columns + col;
        if index >= self.points.len() {
            return None;
        }
        let center = self.point_pos(index, width);
        let dx = pos.x - center.x;
        let dy = pos.y - center.y;
        (dx * dx + dy * dy <= (CELL * 0.45) * (CELL * 0.45)).then_some(index)
    }
}

impl canvas::Program<Message> for SampleMapCanvas {
    type State = SampleMapInteraction;

    fn update(
        &self,
        state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(pos) = cursor.position_in(bounds) else {
                    return (canvas::event::Status::Ignored, None);
                };
                // A press dragged far enough starts a browser drag.
                if let Some((index, origin)) = state.pressed {
                    let dx = pos.x - origin.x;
                    let dy = pos.y - origin.y;
                    if !state.dragged && dx * dx + dy * dy > DRAG_THRESHOLD * DRAG_THRESHOLD {
                        state.dragged = true;
                        let point = &self.points[index];
                        return (
                            canvas::event::Status::Captured,
                            Some(Message::BeginPendingBrowserDrag(
                                point.source.clone(),
                                point.name.clone(),
                            )),
                        );
                    }
                }
                let hovered = self.hit(pos, bounds.width);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    let msg = hovered.map(|index| self.points[index].source.clone());
                    return (
                        canvas::event::Status::Ignored,
                        Some(Message::SampleMapHover(msg)),
                    );
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position_in(bounds) {
                    if let Some(index) = self.hit(pos, bounds.width) {
                        state.pressed = Some((index, pos));
                        state.dragged = false;
                        return (canvas::event::Status::Captured, None);
                    }
                }
                (canvas::event::Status::Ignored, None)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let Some((index, _)) = state.pressed.take() else {
                    return (canvas::event::Status::Ignored, None);
                };
                if state.dragged {
                    return (canvas::event::Status::Captured, None);
                }
                let point = &self.points[index];
                let now = std::time::Instant::now();
                let double = matches!(state.last_click, Some((last_index, last_at))
                    if last_index == index && now.duration_since(last_at).as_millis() <= DOUBLE_CLICK_MS);
                state.last_click = Some((index, now));
                let msg = if double {
                    Message::BrowserEntryDoubleClicked(point.source.clone())
                } else {
                    Message::ClickLocalBrowserEntry(point.source.clone())
                };
                (canvas::event::Status::Captured, Some(msg))
            }
            _ => (canvas::event::Status::Ignored, None),
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), th::bg_surface());
        let width = bounds.width;
        let mut previous_kind: Option<SampleMapKind> = None;
        for (index, point) in self.points.iter().enumerate() {
            let pos = self.point_pos(index, width);
            let dimmed = matches!(self.kind_filter, Some(filter) if filter != point.kind);
            let mut color = point.kind.color();
            if dimmed {
                color.a = 0.18;
            }
            let is_hovered = state.hovered == Some(index);
            let is_selected = self.selected.as_ref() == Some(&point.source);
            let radius = if is_hovered || is_selected { 7.0 } else { 5.0 };
            frame.fill(&canvas::Path::circle(pos, radius), color);
            if is_hovered || is_selected {
                frame.stroke(
                    &canvas::Path::circle(pos, radius + 2.5),
                    canvas::Stroke::default()
                        .with_color(th::text())
                        .with_width(1.5),
                );
            }
            // A kind run starting a row gets its name over the region.
            let starts_row = index % self.columns(width) == 0;
            if Some(point.kind) != previous_kind && starts_row && index > 0 {
                frame.fill_text(canvas::Text {
                    content: point.kind.label().to_string(),
                    position: pos + Vector::new(0.0, -CELL / 2.0 - 2.0),
                    size: iced::Pixels(10.0),
                    color: point.kind.color(),
                    ..Default::default()
                });
            }
            previous_kind = Some(point.kind);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.pressed.is_some() {
            return mouse::Interaction::Grabbing;
        }
        if let Some(pos) = cursor.position_in(bounds) {
            if self.hit(pos, bounds.width).is_some() {
                return mouse::Interaction::Pointer;
            }
        }
        mouse::Interaction::default()
    }
}

impl App {
    /// The sample map for the current browsing scope (query + tag filters
    /// apply), with the kind legend under it.
    pub(super) fn view_browser_sample_map(&self, query: &str) -> Element<'_, Message> {
        let browser = &self.state.browser;
        let mut points: Vec<SampleMapPoint> = browser
            .entries
            .iter()
            .filter(|entry| {
                browser.local_entry_is_result(entry, query)
                    && browser.passes_tag_filters(&entry.source)
            })
            .take(MAX_POINTS)
            .map(|entry| {
                let folders: Vec<&str> = entry
                    .relative_path
                    .parent()
                    .map(|parent| {
                        parent
                            .iter()
                            .rev()
                            .take(2)
                            .filter_map(|part| part.to_str())
                            .collect()
                    })
                    .unwrap_or_default();
                SampleMapPoint {
                    source: entry.source.clone(),
                    name: entry.name.clone(),
                    kind: crate::state::sample_kind_of(&entry.name, &folders),
                }
            })
            .collect();
        points.sort_by(|a, b| {
            (a.kind.rank(), a.name.to_lowercase()).cmp(&(b.kind.rank(), b.name.to_lowercase()))
        });

        let program = SampleMapCanvas {
            selected: browser.selected_source.clone(),
            kind_filter: browser.sample_map_kind_filter,
            points,
        };

        // The legend: one chip per kind present, click filters to it.
        let mut present: Vec<SampleMapKind> = SAMPLE_MAP_KINDS.to_vec();
        present.retain(|kind| program.points.iter().any(|point| point.kind == *kind));
        let mut legend = row![].spacing(4).align_y(iced::Alignment::Center);
        for kind in present {
            let active = browser.sample_map_kind_filter == Some(kind);
            let color = kind.color();
            legend = legend.push(
                button(
                    row![
                        container(iced::widget::Space::with_width(8.0).height(8.0)).style(
                            move |_theme: &Theme| container::Style {
                                background: Some(color.into()),
                                border: iced::Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }
                        ),
                        text(kind.label()).size(9.5).color(if active {
                            th::text()
                        } else {
                            th::text_muted()
                        }),
                    ]
                    .spacing(4)
                    .align_y(iced::Alignment::Center),
                )
                .on_press(Message::Browser(BrowserMsg::ToggleSampleMapKindFilter(
                    kind,
                )))
                .padding([3, 7])
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
                        text_color: if active { th::text() } else { th::text_muted() },
                        border: iced::Border {
                            color: if active { th::accent() } else { th::border() },
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                }),
            );
        }

        column![
            container(canvas(program).width(Length::Fill).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill),
            container(legend)
                .padding(iced::Padding {
                    top: 4.0,
                    right: 4.0,
                    bottom: 2.0,
                    left: 4.0,
                })
                .width(Length::Fill),
        ]
        .spacing(0)
        .into()
    }

    /// The List / Map chips at the RESULTS caption, only while browsing
    /// samples (not a device category or a special place).
    pub(super) fn view_results_view_chips(&self) -> Element<'_, Message> {
        let browser = &self.state.browser;
        let sample_scope = browser.category.is_none() && browser.special_place.is_none();
        if !sample_scope {
            return iced::widget::Space::with_width(0).into();
        }
        let chip = |icon: char,
                    label: &'static str,
                    view: BrowserResultsView,
                    active: bool|
         -> Element<'static, Message> {
            button(
                row![
                    crate::icons::icon(icon).size(11).color(if active {
                        th::accent()
                    } else {
                        th::text_muted()
                    }),
                    text(label).size(9.5).color(if active {
                        th::accent()
                    } else {
                        th::text_muted()
                    }),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::Browser(BrowserMsg::SetResultsView(view)))
            .padding([3, 7])
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
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into()
        };
        row![
            chip(
                crate::icons::LAYOUT_LIST,
                "LIST",
                BrowserResultsView::List,
                browser.results_view == BrowserResultsView::List
            ),
            chip(
                crate::icons::LAYOUT_GRID,
                "MAP",
                BrowserResultsView::SampleMap,
                browser.results_view == BrowserResultsView::SampleMap
            ),
        ]
        .spacing(4)
        .into()
    }
}
