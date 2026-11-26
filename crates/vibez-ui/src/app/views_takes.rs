//! The take lanes shown under an Arrange track.

use iced::widget::{button, canvas, column, container, row, text};
use iced::{Length, Theme};

use crate::domains::arrangement::ArrangementMsg;
use crate::message::Message;
use crate::state::{ProjectTrack, TimelineEditorState};
use crate::theme as th;
use crate::timeline_geometry::TimelineGeometry;
use crate::widgets::timeline::{TakeClip, TakeLaneWidget};
use crate::widgets::track_header::TRACK_HEADER_TOTAL_WIDTH;

use super::*;

/// Height of one take lane.
const TAKE_ROW_H: f32 = 40.0;

impl App {
    /// How much room a track's open take lanes need.
    pub(super) fn take_rows_height(
        &self,
        timeline: &TimelineEditorState,
        track: &ProjectTrack,
    ) -> f32 {
        if !timeline.open_take_lanes.contains(&track.id) {
            return 0.0;
        }
        timeline
            .timeline
            .get(track.id)
            .map_or(0.0, |content| content.take_lanes.len() as f32 * TAKE_ROW_H)
    }

    pub(super) fn push_take_lanes<'a>(
        &'a self,
        mut rows: iced::widget::Column<'a, Message>,
        timeline: &'a TimelineEditorState,
        track: &'a ProjectTrack,
        geometry: TimelineGeometry,
    ) -> iced::widget::Column<'a, Message> {
        if !timeline.open_take_lanes.contains(&track.id) {
            return rows;
        }
        let Some(content) = timeline.timeline.get(track.id) else {
            return rows;
        };
        let color = th::track_color(track.color_index);
        for lane in &content.take_lanes {
            let comped = lane
                .clips
                .iter()
                .flat_map(|take| {
                    content.clips.iter().filter_map(move |clip| {
                        let same_audio = std::sync::Arc::ptr_eq(&clip.audio, &take.audio)
                            && clip.position as i128 - clip.source_offset as i128
                                == take.position as i128 - take.source_offset as i128;
                        let start = clip.position.max(take.position);
                        let end =
                            (clip.position + clip.duration).min(take.position + take.duration);
                        (same_audio && end > start).then_some((start, end))
                    })
                })
                .collect();
            let chip = |label: &'static str, message: ArrangementMsg| {
                button(text(label).size(9))
                    .on_press(Message::Arrangement(message))
                    .padding([1, 5])
                    .style(crate::widgets::control_style::style(None))
            };
            // Using a whole take covers everything its Clips span.
            let span = lane
                .clips
                .iter()
                .map(|clip| (clip.position, clip.position + clip.duration))
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
                .unwrap_or_default();
            let header = container(
                row![
                    iced::widget::Space::with_width(14.0),
                    text(&lane.name)
                        .shaping(crate::typography::USER_TEXT_SHAPING)
                        .size(10)
                        .color(th::text_dim()),
                    iced::widget::horizontal_space(),
                    button(crate::icons::icon(crate::icons::HEADPHONES).size(10))
                        .on_press(Message::Arrangement(ArrangementMsg::AuditionTake(
                            track.id, lane.id
                        )))
                        .padding([1, 5])
                        .style(crate::widgets::control_style::style(
                            timeline
                                .auditioned_take
                                .as_ref()
                                .is_some_and(|heard| {
                                    heard.track_id == track.id && heard.lane == lane.id
                                })
                                .then(th::solo_active)
                        )),
                    chip(
                        "Use",
                        ArrangementMsg::CompFromTake {
                            track_id: track.id,
                            lane: lane.id,
                            start: span.0,
                            end: span.1,
                        }
                    ),
                    chip("x", ArrangementMsg::DeleteTakeLane(track.id, lane.id)),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            )
            .padding([0, 6])
            .width(Length::Fixed(TRACK_HEADER_TOTAL_WIDTH))
            .center_y(Length::Fixed(TAKE_ROW_H))
            .style(|_theme: &Theme| container::Style {
                background: Some(th::bg_surface().into()),
                border: iced::Border {
                    color: th::groove(),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });
            let body = canvas(TakeLaneWidget {
                track_id: track.id,
                lane: lane.id,
                clips: lane
                    .clips
                    .iter()
                    .map(|clip| TakeClip {
                        position: clip.position,
                        duration: clip.duration,
                        source_offset: clip.source_offset,
                        audio: std::sync::Arc::clone(&clip.audio),
                    })
                    .collect(),
                comped,
                color,
                geometry,
                grid: self.state.view.arrange_grid,
                samples_per_beat: self.state.transport.samples_per_beat(),
            })
            .width(Length::Fill)
            .height(Length::Fixed(TAKE_ROW_H));
            rows = rows.push(column![row![header, body]].height(Length::Fixed(TAKE_ROW_H)));
        }
        rows
    }
}
