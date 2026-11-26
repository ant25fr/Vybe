//! Arrangement workspace view: track lanes, clip canvases, per-track
//! automation lanes, and the minimap.
//! Split from views_shell.rs; inherent methods on [`super::App`].

use std::collections::HashSet;

use iced::widget::{
    button, canvas, center, column, container, horizontal_space, mouse_area, row, text,
};
use iced::{Element, Length, Theme};

use crate::domains::browser::BrowserMsg;
use crate::domains::timeline_editor::TimelineEditorAdapter;
use crate::domains::view::ViewMsg;
use vibez_core::id::{ClipId, TrackId};

use crate::icons;
use crate::message::Message;
use crate::state::{ArrangementSelection, ContextMenuTarget, TrackTimelineContent};
use crate::theme as th;
use crate::timeline_geometry::TimelineGeometry;
use crate::timeline_snap::SnapTargets;
use crate::widgets::arrange_corner::{arrange_corner, arrangement_size_labels, song_length_label};
use crate::widgets::group_lane::{GroupLaneWidget, MiniClip};
use crate::widgets::quiet_scroll;
use crate::widgets::timeline::{
    build_row_spans, ArrangementMinimap, LaneGhost, MinimapTrack, NewTrackLane, RulerWidget,
    TimeRulerWidget, TimelineClip, TrackClipCanvas, ARRANGE_RULER_HEIGHT, OVERVIEW_HEIGHT,
    TIME_RULER_HEIGHT,
};
use crate::widgets::track_header::{
    view_editable_channel_name, view_group_header, view_track_header, with_resize_edge,
    ChannelNameStyle, GroupHeaderView, TrackHeaderFlags, TrackHeaderRecordingView,
    TRACK_HEADER_TOTAL_WIDTH,
};

use super::*;
use crate::domains::browser::DragMedia;
use crate::domains::drop_ghost::DropGhost;

/// The height of the row where a drop adds a track.
const NEW_TRACK_ROW_HEIGHT: f32 = 54.0;

/// Expanded return or master automation scrolls inside this cap so the
/// pinned footer can never squeeze the track list out of the window.
const CHANNEL_FOOTER_MAX_HEIGHT: f32 = 240.0;

/// The height of one return or master channel row in the pinned footer.
const CHANNEL_ROW_HEIGHT: f32 = 26.0;

/// The least height the Arrange workspace card keeps when the detail panel
/// below it is dragged tall: the overview, the ruler, one minimum-height
/// track lane, the master channel row and the time ruler, so one track
/// stays visible however tall the rack grows.
pub(super) const ARRANGE_ONE_TRACK_VISIBLE_HEIGHT: f32 = OVERVIEW_HEIGHT
    + ARRANGE_RULER_HEIGHT
    + vibez_project::TrackHeight::MIN.pixels()
    + CHANNEL_ROW_HEIGHT
    + TIME_RULER_HEIGHT;

impl App {
    /// Width of the clip lanes, right of the track headers.
    pub(super) fn arrange_lane_width(&self) -> f32 {
        (self.browser_workspace_width() - crate::widgets::track_header::TRACK_HEADER_TOTAL_WIDTH)
            .max(1.0)
    }

    pub(super) fn view_arrangement(&self) -> Element<'_, Message> {
        let timeline = self.state.arrangement.resolve_timeline().editor;
        if self.state.project_tracks.tracks.is_empty() {
            let browser_drag_active = self.state.browser.drag_source.is_some();
            let ghost = self
                .state
                .browser
                .new_track_drop_ghost(self.state.transport.bpm);
            // The ghost says where the drop lands, so the zone would only
            // repeat it.
            let zone = match self.state.browser.drag_target {
                Some(crate::state::BrowserDropTarget::EmptyArrangement { .. })
                    if ghost.is_none() =>
                {
                    self.state.browser.drop_zone()
                }
                _ => None,
            };
            let prompt_text = if zone.is_some() || ghost.is_some() {
                ""
            } else if self.state.browser.drag_media() == Some(DragMedia::Midi) {
                "Drop for a new MIDI track"
            } else if browser_drag_active {
                "Drop for a new audio track"
            } else {
                "Right-click or Ctrl+T to add a track"
            };
            let prompt = text(prompt_text).size(16).color(th::text_dim());

            let centered = center(prompt).width(Length::Fill).height(Length::Fill);

            use crate::widgets::drop_highlight::{
                drop_highlight, CARD_ZONE_INSET, CARD_ZONE_RADIUS,
            };
            let geometry = self.state.view.viewport.geometry();
            // The new track takes the top row; its lane starts at the area's
            // left edge, where the hover measures beats from.
            let content: Element<'_, Message> = match ghost {
                Some(ghost) => column![
                    container(
                        text(ghost.media.new_track().label())
                            .size(11)
                            .color(th::text())
                    )
                    .padding([6, 8]),
                    new_track_lane(&ghost, geometry),
                ]
                .into(),
                None => centered.into(),
            };
            let surface = container(content)
                .width(Length::Fill)
                .height(Length::FillPortion(5))
                .style(|_theme: &Theme| container::Style {
                    background: Some(th::bg_dark().into()),
                    ..Default::default()
                })
                .into();
            let mut area = mouse_area(drop_highlight(
                surface,
                zone,
                CARD_ZONE_RADIUS,
                CARD_ZONE_INSET,
            ))
            .on_right_press(Message::View(ViewMsg::ShowContextMenu {
                x: self.state.view.cursor_x,
                y: self.state.view.cursor_y,
                target: ContextMenuTarget::ArrangementEmpty,
            }));
            if browser_drag_active {
                let grid = self.state.view.arrange_grid;
                area = area
                    .on_move(move |point| {
                        let beat =
                            grid.snap_beat(geometry.x_to_beat(point.x), geometry.pixels_per_beat());
                        Message::Browser(BrowserMsg::DragHoverEmptyArrangement {
                            beat: beat.max(0.0),
                        })
                    })
                    .on_exit(Message::Browser(BrowserMsg::LeaveDragTarget(
                        crate::state::BrowserDropTarget::EmptyArrangement { beat: 0.0 },
                    )))
                    .on_release(Message::DropSampleOnEmptyArrangement);
            }
            return super::desktop_pointer::drop_target(
                area,
                crate::domains::desktop_pointer::PointerTarget::NewTrackRow,
            );
        }

        let playhead_beats = self.state.transport.position_beats();
        let sample_rate = self.state.transport.sample_rate;
        let bpm = self.state.transport.bpm;
        let zoom_level = self.state.view.viewport.zoom_level;
        let scroll_offset = self.state.view.viewport.scroll_offset_beats;
        let geometry = TimelineGeometry::from_zoom(zoom_level, scroll_offset);
        let total_beats = self.state.total_beats();

        let play_start_beats = self.state.transport.play_start_beats;
        let snap_targets = std::sync::Arc::new(SnapTargets::for_arrangement(
            timeline,
            &self.state.transport,
        ));

        // Beat-based ruler, offset by track header width
        let ruler = RulerWidget {
            meter: self.state.transport.meter,
            wheel_zoom: self.wheel_zoom(),
            playhead_beats,
            play_start_beats: Some(play_start_beats),
            zoom_level,
            grid: self.state.view.arrange_grid,
            scroll_offset_beats: scroll_offset,
            loop_brace: true,
            loop_enabled: self.state.transport.loop_enabled,
            loop_start_beats: self.state.transport.loop_start_beats,
            loop_end_beats: self.state.transport.loop_end_beats,
            time_selection_active: timeline.time_selection_active,
            selection_start_beats: timeline.selection_start_beats,
            selection_end_beats: timeline.selection_end_beats,
            snap_targets: snap_targets.clone(),
        };
        let ruler_canvas: Element<'_, Message> = canvas(ruler)
            .width(Length::Fill)
            .height(Length::Fixed(ARRANGE_RULER_HEIGHT))
            .into();

        let (tracks_label, clips_label) = arrangement_size_labels(
            self.state.project_tracks.tracks.len(),
            timeline.timeline.clip_count(
                self.state
                    .project_tracks
                    .tracks
                    .iter()
                    .map(|track| track.id),
            ),
        );
        let ruler_spacer = arrange_corner(
            tracks_label,
            clips_label,
            TRACK_HEADER_TOTAL_WIDTH,
            ARRANGE_RULER_HEIGHT,
        );

        let ruler_row = row![ruler_spacer, ruler_canvas];

        // Arrangement overview minimap
        let spb = if bpm > 0.0 {
            60.0 * sample_rate as f64 / bpm
        } else {
            1.0
        };
        let minimap = ArrangementMinimap {
            total_beats,
            scroll_offset_beats: scroll_offset,
            zoom_level,
            playhead_beats,
            bpm,
            loop_enabled: self.state.transport.loop_enabled,
            loop_start_beats: self.state.transport.loop_start_beats,
            loop_end_beats: self.state.transport.loop_end_beats,
            wheel_zoom: self.wheel_zoom(),
            tracks: self
                .state
                .project_tracks
                .tracks
                .iter()
                .map(|t| {
                    let color = th::track_color(t.color_index);
                    let content = timeline.timeline.get(t.id);
                    let mut clips: Vec<(f64, f64)> = content
                        .into_iter()
                        .flat_map(|content| {
                            content
                                .clips
                                .iter()
                                .map(|c| (c.position as f64 / spb, c.duration as f64 / spb))
                        })
                        .collect();
                    clips.extend(content.into_iter().flat_map(|content| {
                        content
                            .note_clips
                            .iter()
                            .map(|c| (c.position_beats, c.duration_beats))
                    }));
                    MinimapTrack { color, clips }
                })
                .collect(),
        };
        let minimap_spacer = arrange_corner(
            "Overview".into(),
            song_length_label(self.state.duration_seconds(), bpm),
            TRACK_HEADER_TOTAL_WIDTH,
            OVERVIEW_HEIGHT,
        );
        let minimap_canvas: Element<'_, Message> = canvas(minimap)
            .width(Length::Fill)
            .height(Length::Fixed(OVERVIEW_HEIGHT))
            .into();
        let minimap_row = row![minimap_spacer, minimap_canvas];

        // Collect track IDs and kinds for cross-track drag
        // Tracks inside a folded group take no row.
        let project = &self.state.project_tracks;
        let shown: Vec<&crate::state::ProjectTrack> = project
            .tracks
            .iter()
            .filter(|track| !project.is_folded_away(track.id))
            .collect();
        let track_ids: Vec<TrackId> = shown.iter().map(|t| t.id).collect();
        // The group rows above each shown track. A folded group's row waits
        // for the next shown track, or the end of the list.
        let mut listed = Vec::new();
        let mut waiting: Vec<TrackId> = Vec::new();
        let mut headers: Vec<Vec<TrackId>> = Vec::with_capacity(shown.len());
        for track in &project.tracks {
            let mut above: Vec<TrackId> = project.ancestors(track.id);
            above.reverse();
            above.retain(|group| !listed.contains(group) && !project.is_folded_away(*group));
            listed.extend(above.iter().copied());
            waiting.extend(above);
            if !project.is_folded_away(track.id) {
                headers.push(std::mem::take(&mut waiting));
            }
        }

        // Track rows: header widgets + clip canvas
        // Real vertical layout of the column, so a rubber-band drag can map
        // a y offset back to a track even with automation lanes expanded.
        let mut row_spans = build_row_spans(shown.iter().map(|track| {
            let automation = if self.state.automation_ui.expanded.contains(&track.id) {
                self.automation_rows_height(timeline, track.id)
            } else {
                0.0
            };
            let extra_height = automation + self.take_rows_height(timeline, track);
            (
                track.id,
                self.state.track_heights.height(track.id).pixels(),
                extra_height,
            )
        }));

        // Lanes sit lower by the group rows above them.
        let mut lowered = 0.0;
        for (span, above) in row_spans.iter_mut().zip(&headers) {
            lowered += above
                .iter()
                .map(|&group| self.group_row_height(timeline, group))
                .sum::<f32>();
            span.top += lowered;
        }

        let mut track_rows = column![].spacing(0);
        let empty_content = TrackTimelineContent::default();
        let browser_drag_active = self.state.browser.drag_source.is_some();

        for ((track, span), above) in shown.iter().copied().zip(&row_spans).zip(&headers) {
            for &group in above {
                track_rows = self.push_group_row(track_rows, timeline, group);
            }
            let content = timeline.timeline.get(track.id).unwrap_or(&empty_content);
            // A frozen track shows the audio it plays; its clips are set
            // aside.
            let frozen_content = track.frozen_clip.map(|frozen| TrackTimelineContent {
                clips: content
                    .clips
                    .iter()
                    .filter(|clip| clip.id == frozen)
                    .cloned()
                    .collect(),
                automation: content.automation.clone(),
                ..TrackTimelineContent::default()
            });
            let content = frozen_content.as_ref().unwrap_or(content);
            let selected = timeline.selected_track == Some(track.id)
                || self.state.arrangement.selected_channels.contains(&track.id);
            let track_color = th::track_color(track.color_index);

            // Collect selected clip IDs for this track
            let selected_clips: HashSet<ClipId> = timeline
                .selected_clips
                .iter()
                .filter_map(|sel| match sel {
                    ArrangementSelection::AudioClip { track_id, clip_id }
                        if *track_id == track.id =>
                    {
                        Some(*clip_id)
                    }
                    ArrangementSelection::NoteClip { track_id, clip_id }
                        if *track_id == track.id =>
                    {
                        Some(*clip_id)
                    }
                    _ => None,
                })
                .collect();

            // Track header (iced widgets)
            let editing = self.state.view.editing_track_name == Some(track.id);
            let automation_open = self.state.automation_ui.expanded.contains(&track.id);
            let input_channels = self.state.audio_settings.input_channel_count();
            let input_target = self.state.audio_recording.armed_track == Some(track.id)
                || self.state.audio_recording.monitor_track == Some(track.id);
            let header = with_resize_edge(
                view_track_header(
                    track,
                    TrackHeaderFlags {
                        selected,
                        editing_name: editing,
                        automation_open,
                        hovered: self.state.view.hovered_track_header == Some(track.id),
                    },
                    &self.state.view.edit_name_text,
                    TrackHeaderRecordingView {
                        output_peaks: self
                            .track_meter_peaks
                            .get(&track.id)
                            .copied()
                            .unwrap_or_default(),
                        input_channels,
                        armed: self.state.audio_recording.armed_track == Some(track.id),
                        input_peaks: input_target.then_some((
                            self.state.audio_recording.input_peak_l,
                            self.state.audio_recording.input_peak_r,
                        )),
                        source_tracks: &self.state.project_tracks.tracks,
                    },
                ),
                track.id,
                self.state.track_heights.resizing() == Some(track.id),
            );

            // Clip canvas for this track
            let refusal = self.state.browser.lane_refusal(track.kind);
            let mut clip_canvas_widget = {
                let mut canvas = TrackClipCanvas::from_track(
                    track,
                    content,
                    playhead_beats,
                    zoom_level,
                    self.state.view.arrange_grid,
                    scroll_offset,
                    (self.state.view.window_width
                        - crate::widgets::track_header::TRACK_HEADER_TOTAL_WIDTH)
                        .max(1.0),
                    total_beats,
                    sample_rate,
                    selected,
                    track_color,
                    bpm,
                    track.id,
                    track_ids.clone(),
                    selected_clips,
                    self.state.transport.loop_enabled,
                    self.state.transport.loop_start_beats,
                    self.state.transport.loop_end_beats,
                    timeline.time_selection_active,
                    timeline.selection_start_beats,
                    timeline.selection_end_beats,
                    timeline.time_selection_track,
                    browser_drag_active,
                )
                .with_marquee(
                    row_spans.clone(),
                    timeline.marquee.as_ref().map(|marquee| marquee.rect),
                )
                .with_snap_targets(snap_targets.clone())
                .with_clip_drag(timeline.clip_drag.as_ref().map(|drag| drag.preview.clone()))
                .with_drop_zone(self.state.browser.lane_drop_zone(track.id))
                .with_drop_ghost(
                    self.state
                        .browser
                        .lane_drop_ghost(track.id, bpm)
                        .map(|ghost| LaneGhost::new(&ghost, geometry.pixels_per_beat())),
                )
                .with_drop_refusal(refusal.clone());
                canvas.wheel_zoom = self.wheel_zoom();
                canvas
            };
            if let Some(preview) = self.state.audio_recording.preview_for_track(track.id) {
                let name = match preview.source {
                    crate::domains::audio_recording::AudioRecordingSource::HardwareInput => {
                        "RECORDING INPUT".into()
                    }
                    crate::domains::audio_recording::AudioRecordingSource::TrackOutput(source) => {
                        let source_name = self
                            .state
                            .find_track(source)
                            .map_or("MIDI TRACK", |track| track.name.as_str());
                        format!("RESAMPLING {source_name}")
                    }
                };
                clip_canvas_widget =
                    clip_canvas_widget.with_audio_recording_preview(TimelineClip {
                        color: None,
                        clip_id: preview.clip_id,
                        position: preview.position,
                        duration: preview.duration,
                        name,
                        peaks: preview.peaks,
                        peak_span_frames: Some(preview.frames_per_peak),
                        loop_enabled: false,
                        loop_start: 0,
                        loop_end: 0,
                        gain_db: 0.0,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                        fade_in_curve: Default::default(),
                        fade_out_curve: Default::default(),
                        crossfade_in_from: None,
                        crossfade_out_to: None,
                        warp_stale: false,
                    });
            }
            let track_id = track.id;
            let compatible = refusal.is_none();
            let grid = self.state.view.arrange_grid;
            let track_geometry = geometry;
            let mut clip_canvas_area = mouse_area(
                canvas(clip_canvas_widget)
                    .width(Length::Fill)
                    .height(Length::Fixed(span.lane_height)),
            );
            if browser_drag_active {
                clip_canvas_area = clip_canvas_area
                    .on_move(move |point| {
                        let beat = grid.snap_beat(
                            track_geometry.x_to_beat(point.x),
                            track_geometry.pixels_per_beat(),
                        );
                        Message::Browser(BrowserMsg::DragHoverTrack {
                            track_id,
                            beat: beat.max(0.0),
                            compatible,
                        })
                    })
                    .on_exit(Message::Browser(BrowserMsg::ClearDragTarget));
            }
            let clip_canvas_area: Element<'_, Message> = if track.frozen_clip.is_some() {
                iced::widget::stack![
                    clip_canvas_area,
                    iced::widget::opaque(
                        container(text("Frozen").size(10).color(th::text_dim()))
                            .padding([2, 6])
                            .width(Length::Fill)
                            .height(Length::Fixed(span.lane_height))
                            .style(|_theme: &Theme| container::Style {
                                background: Some(th::with_alpha(th::bg_dark(), 0.25).into()),
                                ..Default::default()
                            })
                    )
                ]
                .into()
            } else {
                clip_canvas_area.into()
            };
            let clip_canvas = super::desktop_pointer::drop_target(
                clip_canvas_area,
                crate::domains::desktop_pointer::PointerTarget::Lane(track_id),
            );

            let header = drop_cue(header, self.drop_would_move(track.id));
            let header = mouse_area(header)
                .on_release(Message::HeaderReleased(track.id))
                .on_right_press(Message::View(ViewMsg::ShowContextMenu {
                    x: self.state.view.cursor_x,
                    y: self.state.view.cursor_y,
                    target: ContextMenuTarget::TrackHeader { track_id: track.id },
                }));
            let track_row = row![header, clip_canvas].height(Length::Fixed(span.lane_height));

            track_rows = track_rows.push(track_row);
            track_rows = self.push_take_lanes(track_rows, timeline, track, geometry);

            if automation_open {
                track_rows = self.push_automation_lanes(track_rows, timeline, track, track_color);
            }
        }

        // Folded groups after the last shown track, then empty ones.
        for &group in &waiting {
            track_rows = self.push_group_row(track_rows, timeline, group);
        }
        for bus in &self.state.project_tracks.buses {
            if bus.is_group
                && !listed.contains(&bus.id)
                && !self.state.project_tracks.is_folded_away(bus.id)
            {
                track_rows = self.push_group_row(track_rows, timeline, bus.id);
            }
        }

        if self.state.browser.drag_source.is_some() {
            track_rows =
                track_rows.push(self.view_new_track_row(TRACK_HEADER_TOTAL_WIDTH, geometry));
        }

        // ── Returns + master: automation-only channels ──
        // Clipless lanes at the bottom, Ableton-style: a slim header
        // (select, expand, delete for returns) and their automation
        // lanes when expanded.
        let mut channel_rows = column![].spacing(0);
        let master_ref = &self.state.project_tracks.master;
        let channel_refs: Vec<&crate::state::ProjectTrack> = self
            .state
            .project_tracks
            .buses
            .iter()
            // A group has its row among the tracks.
            .filter(|bus| !bus.is_group)
            .chain(std::iter::once(master_ref))
            .collect();
        for channel in channel_refs {
            let is_master = channel.id.is_master();
            let chan_color = if is_master {
                th::accent()
            } else {
                th::track_color(channel.color_index)
            };
            let selected = timeline.selected_track == Some(channel.id);
            let expanded = self.state.automation_ui.expanded.contains(&channel.id);

            let toggle = button(
                icons::icon(icons::SLIDERS_VERTICAL)
                    .size(10)
                    .color(if expanded {
                        th::accent()
                    } else {
                        th::text_dim()
                    }),
            )
            .on_press(Message::Automation(
                crate::domains::automation::AutomationMsg::ToggleTrackLanes(channel.id),
            ))
            .padding([2, 4])
            .style(|_theme: &Theme, _status| button::Style {
                background: None,
                text_color: th::text_dim(),
                border: iced::Border::default(),
                ..Default::default()
            });

            let dot = text("\u{25CF}").size(9).color(chan_color);
            let name_color = if selected { th::text() } else { th::text_dim() };
            let name: Element<'_, Message> = if is_master {
                text(&channel.name)
                    .shaping(crate::typography::USER_TEXT_SHAPING)
                    .size(11)
                    .color(name_color)
                    .into()
            } else {
                view_editable_channel_name(
                    channel,
                    (self.state.view.editing_track_name == Some(channel.id))
                        .then_some(self.state.view.edit_name_text.as_str()),
                    ChannelNameStyle {
                        size: 11,
                        font: iced::Font::DEFAULT,
                        color: name_color,
                    },
                )
            };

            let remove_el: Element<'_, Message> = if is_master {
                text("").size(9).into()
            } else {
                button(icons::icon(icons::TRASH_2).size(9).color(th::text_dim()))
                    .on_press(Message::remove_bus(channel.id))
                    .padding([1, 4])
                    .style(|_theme: &Theme, status| {
                        let tc = match status {
                            button::Status::Hovered | button::Status::Pressed => th::danger(),
                            _ => th::text_dim(),
                        };
                        button::Style {
                            background: None,
                            text_color: tc,
                            border: iced::Border::default(),
                            ..Default::default()
                        }
                    })
                    .into()
            };
            let header_row = row![toggle, dot, name, horizontal_space(), remove_el]
                .spacing(6)
                .align_y(iced::Alignment::Center);

            let header: Element<'_, Message> = mouse_area(
                container(header_row)
                    .padding([0, 8])
                    .width(Length::Fixed(
                        crate::widgets::track_header::TRACK_HEADER_TOTAL_WIDTH,
                    ))
                    .height(Length::Fixed(CHANNEL_ROW_HEIGHT))
                    .align_y(iced::alignment::Vertical::Center)
                    .style(move |_theme: &Theme| container::Style {
                        background: Some(if selected {
                            th::track_bg_selected().into()
                        } else {
                            th::bg_surface().into()
                        }),
                        border: iced::Border {
                            color: th::groove(),
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    }),
            )
            .on_press(Message::select_track(channel.id))
            .into();

            let filler = container(column![])
                .width(Length::Fill)
                .height(Length::Fixed(CHANNEL_ROW_HEIGHT))
                .style(|_theme: &Theme| container::Style {
                    background: Some(th::bg_dark().into()),
                    border: iced::Border {
                        color: th::groove(),
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                });

            channel_rows =
                channel_rows.push(row![header, filler].height(Length::Fixed(CHANNEL_ROW_HEIGHT)));
            if expanded {
                channel_rows =
                    self.push_automation_lanes(channel_rows, timeline, channel, chan_color);
            }
        }

        let track_body = quiet_scroll::vertical_above(
            track_rows,
            self.view_new_track_space(TRACK_HEADER_TOTAL_WIDTH, geometry),
        );
        let channel_footer =
            container(quiet_scroll::vertical(channel_rows)).max_height(CHANNEL_FOOTER_MAX_HEIGHT);

        let time_ruler: Element<'_, Message> = canvas(TimeRulerWidget {
            bpm,
            zoom_level,
            scroll_offset_beats: scroll_offset,
        })
        .width(Length::Fill)
        .height(Length::Fixed(TIME_RULER_HEIGHT))
        .into();
        let time_row = row![
            arrange_corner(
                "Time".into(),
                String::new(),
                TRACK_HEADER_TOTAL_WIDTH,
                TIME_RULER_HEIGHT
            ),
            time_ruler
        ];

        let content = column![minimap_row, ruler_row, track_body, channel_footer, time_row];

        // mouse_area only provides on_right_press (no cursor position),
        // so use the globally tracked cursor for the scrollable background.
        // Track canvas right-clicks still supply their precise event position.
        mouse_area(
            container(content)
                .width(Length::Fill)
                .height(Length::FillPortion(5))
                .style(|_theme: &Theme| container::Style {
                    background: Some(th::bg_dark().into()),
                    ..Default::default()
                }),
        )
        .on_right_press(Message::View(ViewMsg::ShowContextMenu {
            x: self.state.view.cursor_x,
            y: self.state.view.cursor_y,
            target: ContextMenuTarget::ArrangementEmpty,
        }))
        .into()
    }
}

impl App {
    /// The row under the last track where a drop adds a track, its header
    /// `header_width` wide and its lane laid out by `geometry`.
    pub(super) fn view_new_track_row(
        &self,
        header_width: f32,
        geometry: TimelineGeometry,
    ) -> Element<'_, Message> {
        let grid = self.state.view.arrange_grid;
        let bpm = self.state.transport.bpm;
        let zone = match self.state.browser.drag_target {
            Some(crate::state::BrowserDropTarget::EmptyArrangement { .. }) => {
                self.state.browser.drop_zone()
            }
            _ => None,
        };
        // Until the drag reaches it, the lane only hints at what it is;
        // once there, its zone says it.
        let hint = match (zone.is_some(), self.state.browser.new_track_kind()) {
            (false, Some(kind)) => kind.label(),
            _ => "",
        };
        let ghost = self.state.browser.new_track_drop_ghost(bpm);
        let header_text = match &ghost {
            Some(ghost) => text(ghost.media.new_track().label())
                .size(11)
                .color(th::text()),
            None => text("+").size(14).color(th::accent()),
        };
        let header = container(header_text)
            .width(Length::Fixed(header_width))
            .height(Length::Fixed(NEW_TRACK_ROW_HEIGHT))
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(|_theme: &Theme| container::Style {
                background: Some(th::bg_surface().into()),
                border: iced::Border {
                    color: th::divider(),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });
        use crate::widgets::drop_highlight::{drop_highlight, CARD_ZONE_INSET, CARD_ZONE_RADIUS};
        let lane = container(text(hint).size(12).color(th::text_muted()))
            .width(Length::Fill)
            .height(Length::Fixed(NEW_TRACK_ROW_HEIGHT))
            .center_x(Length::Fill)
            .center_y(Length::Fixed(NEW_TRACK_ROW_HEIGHT));
        let lane: Element<'_, Message> = match ghost {
            Some(ghost) => new_track_lane(&ghost, geometry),
            None => drop_highlight(lane.into(), zone, CARD_ZONE_RADIUS, CARD_ZONE_INSET),
        };
        row![
            new_track_gutter_area(header, grid, geometry),
            new_track_drop_area(lane, grid, geometry),
        ]
        .height(Length::Fixed(NEW_TRACK_ROW_HEIGHT))
        .into()
    }

    /// The space under the new-track row, which takes a drop as the row
    /// does, the lane at the pointer's beat and the gutter `header_width`
    /// wide under the headers at the lane's start.
    pub(super) fn view_new_track_space(
        &self,
        header_width: f32,
        geometry: TimelineGeometry,
    ) -> Element<'_, Message> {
        let space = iced::widget::Space::new(Length::Fill, Length::Fill);
        if self.state.browser.drag_source.is_none() {
            return space.into();
        }
        let grid = self.state.view.arrange_grid;
        row![
            new_track_gutter_area(
                iced::widget::Space::new(Length::Fixed(header_width), Length::Fill),
                grid,
                geometry,
            ),
            new_track_drop_area(space, grid, geometry),
        ]
        .height(Length::Fill)
        .into()
    }
}

/// `content` taking a drop for a new track at the beat under the pointer,
/// its left edge where `geometry` starts.
fn new_track_drop_area<'a>(
    content: impl Into<Element<'a, Message>>,
    grid: crate::state::GridConfig,
    geometry: TimelineGeometry,
) -> Element<'a, Message> {
    new_track_drop(
        content,
        crate::domains::desktop_pointer::PointerTarget::NewTrackRow,
        move |x| grid.snap_beat(geometry.x_to_beat(x), geometry.pixels_per_beat()),
    )
}

/// `content` under the track headers taking a drop for a new track at the
/// beat where the lane starts.
fn new_track_gutter_area<'a>(
    content: impl Into<Element<'a, Message>>,
    grid: crate::state::GridConfig,
    geometry: TimelineGeometry,
) -> Element<'a, Message> {
    let beat = grid.snap_beat(geometry.x_to_beat(0.0), geometry.pixels_per_beat());
    new_track_drop(
        content,
        crate::domains::desktop_pointer::PointerTarget::NewTrackGutter,
        move |_| beat,
    )
}

/// `content` tagged `target`, taking a drop for a new track at the beat
/// `beat_at` gives for the pointer's x.
fn new_track_drop<'a>(
    content: impl Into<Element<'a, Message>>,
    target: crate::domains::desktop_pointer::PointerTarget,
    beat_at: impl Fn(f32) -> f64 + 'a,
) -> Element<'a, Message> {
    let area = mouse_area(content)
        .on_move(move |point| {
            Message::Browser(BrowserMsg::DragHoverEmptyArrangement {
                beat: beat_at(point.x).max(0.0),
            })
        })
        .on_exit(Message::Browser(BrowserMsg::LeaveDragTarget(
            crate::state::BrowserDropTarget::EmptyArrangement { beat: 0.0 },
        )))
        .on_release(Message::DropSampleOnEmptyArrangement);
    super::desktop_pointer::drop_target(area, target)
}

/// The lane of the track a drop adds, with `ghost` in it.
fn new_track_lane<'a>(ghost: &DropGhost<'_>, geometry: TimelineGeometry) -> Element<'a, Message> {
    canvas(NewTrackLane {
        ghost: LaneGhost::new(ghost, geometry.pixels_per_beat()),
        geometry,
    })
    .width(Length::Fill)
    .height(Length::Fixed(NEW_TRACK_ROW_HEIGHT))
    .into()
}

/// `header` outlined while a dragged header would land on it.
fn drop_cue<'a>(header: impl Into<Element<'a, Message>>, shown: bool) -> Element<'a, Message> {
    let header = header.into();
    if !shown {
        return header;
    }
    container(header)
        .padding(2)
        .style(|_theme: &Theme| container::Style {
            background: Some(th::accent().into()),
            ..Default::default()
        })
        .into()
}

/// The Clips of every track under `group`, one row a track, as the group's
/// lane summarises them.
fn group_clip_rows(
    project: &crate::state::ProjectTracksState,
    timeline: &crate::state::TimelineEditorState,
    group: TrackId,
    samples_per_beat: f64,
) -> Vec<Vec<MiniClip>> {
    project
        .tracks
        .iter()
        .filter(|track| project.ancestors(track.id).contains(&group))
        .map(|track| {
            let Some(content) = timeline.timeline.get(track.id) else {
                return Vec::new();
            };
            let color = |own: Option<u8>| {
                let color = th::track_color(own.unwrap_or(track.color_index));
                if track.mute {
                    th::with_alpha(color, 0.35)
                } else {
                    color
                }
            };
            // A frozen track shows the audio it plays alone.
            let shown = |id: ClipId| track.frozen_clip.is_none_or(|frozen| frozen == id);
            let audio = content
                .clips
                .iter()
                .filter(|clip| shown(clip.id) && samples_per_beat > 0.0)
                .map(|clip| MiniClip {
                    start_beats: clip.position as f64 / samples_per_beat,
                    length_beats: clip.duration as f64 / samples_per_beat,
                    color: color(clip.color_index),
                });
            let notes = content
                .note_clips
                .iter()
                .filter(|clip| shown(clip.id))
                .map(|clip| MiniClip {
                    start_beats: clip.position_beats,
                    length_beats: clip.duration_beats,
                    color: color(clip.color_index),
                });
            audio.chain(notes).collect()
        })
        .collect()
}

impl App {
    /// A group's row with its automation lanes under it.
    fn push_group_row<'a>(
        &'a self,
        mut rows: iced::widget::Column<'a, Message>,
        timeline: &'a crate::state::TimelineEditorState,
        group: TrackId,
    ) -> iced::widget::Column<'a, Message> {
        rows = rows.push(self.view_group_row(timeline, group));
        let bus = self
            .state
            .project_tracks
            .buses
            .iter()
            .find(|b| b.id == group);
        match bus {
            Some(bus) if self.state.automation_ui.expanded.contains(&group) => {
                self.push_automation_lanes(rows, timeline, bus, th::track_color(bus.color_index))
            }
            _ => rows,
        }
    }

    /// How much room a group's row takes, open automation lanes included.
    fn group_row_height(
        &self,
        timeline: &crate::state::TimelineEditorState,
        group: TrackId,
    ) -> f32 {
        let lanes = if self.state.automation_ui.expanded.contains(&group) {
            self.automation_rows_height(timeline, group)
        } else {
            0.0
        };
        self.state.track_heights.height(group).pixels() + lanes
    }

    /// A group's row: a header like a track's, beside its tracks' Clips in
    /// miniature.
    fn view_group_row<'a>(
        &'a self,
        timeline: &'a crate::state::TimelineEditorState,
        group: TrackId,
    ) -> Element<'a, Message> {
        let project = &self.state.project_tracks;
        let Some(bus) = project.buses.iter().find(|bus| bus.id == group) else {
            return column![].into();
        };
        let selected = timeline.selected_track == Some(group)
            || self.state.arrangement.selected_channels.contains(&group);
        let height = self.state.track_heights.height(group).pixels();
        let header = view_group_header(
            bus,
            GroupHeaderView {
                flags: TrackHeaderFlags {
                    selected,
                    editing_name: self.state.view.editing_track_name == Some(group),
                    automation_open: self.state.automation_ui.expanded.contains(&group),
                    hovered: self.state.view.hovered_track_header == Some(group),
                },
                peaks: self
                    .track_meter_peaks
                    .get(&group)
                    .copied()
                    .unwrap_or_default(),
                depth: project.ancestors(group).len(),
            },
            &self.state.view.edit_name_text,
        );
        let header = with_resize_edge(
            header,
            group,
            self.state.track_heights.resizing() == Some(group),
        );
        let menu = Message::View(ViewMsg::ShowContextMenu {
            x: self.state.view.cursor_x,
            y: self.state.view.cursor_y,
            target: ContextMenuTarget::TrackHeader { track_id: group },
        });
        let header = mouse_area(drop_cue(header, self.drop_would_move(group)))
            .on_release(Message::HeaderReleased(group))
            .on_right_press(menu.clone());
        let lane = canvas(GroupLaneWidget {
            group,
            rows: group_clip_rows(
                project,
                timeline,
                group,
                self.state.transport.samples_per_beat(),
            ),
            geometry: TimelineGeometry::from_zoom(
                self.state.view.viewport.zoom_level,
                self.state.view.viewport.scroll_offset_beats,
            ),
            playhead_beats: self.state.transport.position_beats(),
            selected,
            meter: self.state.transport.meter,
            wheel_zoom: self.wheel_zoom(),
        })
        .width(Length::Fill)
        .height(Length::Fixed(height));
        row![header, mouse_area(lane).on_right_press(menu)]
            .height(Length::Fixed(height))
            .into()
    }
}

#[cfg(test)]
mod group_lane_rows_tests {
    use super::*;
    use crate::state::{ProjectTrack, UiClip};

    #[test]
    fn a_group_lane_shows_one_row_per_track_with_its_clips_in_place() {
        let mut app = super::super::test_support::app();
        let mut group = ProjectTrack::new(TrackId::new(), "Group 1".into(), 0);
        group.is_group = true;
        let group_id = group.id;
        let mut kick = ProjectTrack::new(TrackId::new(), "Kick".into(), 1);
        let mut bass = ProjectTrack::new(TrackId::new(), "Bass".into(), 2);
        let outside = ProjectTrack::new(TrackId::new(), "Vox".into(), 3);
        kick.output = Some(group_id);
        bass.output = Some(group_id);
        let kick_id = kick.id;
        let project = std::sync::Arc::make_mut(&mut app.state.project_tracks);
        project.tracks.extend([kick, bass, outside]);
        project.buses.push(group);
        let mut clip = UiClip::test_clip(2_000, 1_000);
        clip.color_index = Some(5);
        app.state.arrange_content_mut(kick_id).clips.push(clip);

        let rows = group_clip_rows(
            &app.state.project_tracks,
            &app.state.arrangement,
            group_id,
            1_000.0,
        );

        assert_eq!(
            rows,
            [
                vec![MiniClip {
                    start_beats: 2.0,
                    length_beats: 1.0,
                    color: th::track_color(5),
                }],
                Vec::new(),
            ]
        );
    }
}
