//! Split out of app.rs; inherent methods on [`super::App`].

use iced::widget::{
    button, canvas, center, column, container, horizontal_space, pick_list, row, text,
};
use iced::{Color, Element, Length, Theme};

use crate::domains::piano_roll::PianoRollMsg;
use crate::domains::view::{GridTarget, ViewMsg};
use vibez_core::id::{ClipId, SectionId, TrackId};

use crate::icons;
use crate::message::Message;
use crate::shell_layout::DETAIL_PANEL_MIN_HEIGHT;
use crate::state::{ArrangementSelection, DetailPanelTab, GridCommand, TimelineEditorState};
use crate::theme as th;
use crate::widgets::audio_clip_detail::audio_clip_editor_view;
use crate::widgets::clip_editor_view::ClipEditorView;
use crate::widgets::piano_roll::{PianoRollWidget, VelocityLaneWidget};

use super::*;

const DETAIL_CARD_PADDING: f32 = 10.0;
const AUDIO_DETAIL_PANEL_PREFERRED_HEIGHT: f32 = 260.0;
const MIDI_DETAIL_PANEL_PREFERRED_HEIGHT: f32 = 360.0;
// The chrome row above the work column, the status bar below it and the
// frame gaps around them, which a dragged detail panel always leaves alone.
const APP_CHROME_AND_GAPS_HEIGHT: f32 = super::views_control_bar_style::BAR_HEIGHT
    + crate::shell_layout::STATUS_HEIGHT
    + 2.0 * crate::shell_layout::PANE_GAP;
// The room the workspace card keeps at least outside Arrange, where the pad
// surface needs its height. In Arrange the rack may grow until the card is
// down to one visible track, so the drag never crushes the timeline into a
// letterbox either.
const WORKSPACE_MIN_HEIGHT: f32 = 440.0;
// The dogfood reference at a 1,920 x 1,040 work area leaves about 364 px for
// the detail panel. Keep that 35% composition for the panel's resting height;
// a dragged height may use all the room the window has, so the rack can be
// enlarged past it in Arrange.
const DETAIL_PANEL_MAX_WINDOW_FRACTION: f32 = 0.35;

pub(super) fn resolved_detail_playhead_samples(
    editing_perform: bool,
    selected_section: Option<SectionId>,
    playing_section: Option<SectionId>,
    arrange_samples: u64,
    section_samples: u64,
) -> Option<u64> {
    if !editing_perform {
        return Some(arrange_samples);
    }
    selected_section
        .filter(|selected| Some(*selected) == playing_section)
        .map(|_| section_samples)
}

/// The tallest the detail card may get in a window `window_height` tall: all
/// the room the window leaves below the workspace card's `workspace_min`
/// minimum, never below the card's minimum. A drag may use this whole
/// range, so in Arrange the rack can be enlarged until one track stays
/// visible.
fn detail_panel_maximum_height(window_height: f32, workspace_min: f32) -> f32 {
    (window_height - APP_CHROME_AND_GAPS_HEIGHT - workspace_min).max(DETAIL_PANEL_MIN_HEIGHT)
}

/// The tallest the detail card rests at before any drag: the window's room
/// for it, capped at the 35% composition so the workspace keeps its share.
fn detail_panel_resting_maximum_height(window_height: f32, workspace_min: f32) -> f32 {
    detail_panel_maximum_height(window_height, workspace_min)
        .min(window_height * DETAIL_PANEL_MAX_WINDOW_FRACTION)
        .max(DETAIL_PANEL_MIN_HEIGHT)
}

/// The detail card's height from the height `stored` the producer dragged,
/// if any. Until the divider is first dragged (`None`) the card rests at
/// the default, grown to an open editor's preferred height, within the 35%
/// composition; a dragged height is honoured over the whole range the
/// window allows, so the rack/piano roll can be enlarged or shrunk freely
/// even while the piano roll is open.
fn effective_detail_panel_height(
    stored: Option<f32>,
    window_height: f32,
    workspace_min: f32,
    editor_preferred_height: f32,
) -> f32 {
    match stored {
        Some(height) => height.clamp(
            DETAIL_PANEL_MIN_HEIGHT,
            detail_panel_maximum_height(window_height, workspace_min),
        ),
        None => crate::state::DETAIL_PANEL_DEFAULT_HEIGHT
            .max(editor_preferred_height)
            .clamp(
                DETAIL_PANEL_MIN_HEIGHT,
                detail_panel_resting_maximum_height(window_height, workspace_min),
            ),
    }
}

fn visible_note_clip_for_track(editor: &TimelineEditorState, track_id: TrackId) -> Option<ClipId> {
    editor
        .selected_note_clip
        .filter(|(selected_track, _)| *selected_track == track_id)
        .map(|(_, clip_id)| clip_id)
        .or_else(|| {
            editor.selected_clips.iter().find_map(|selection| {
                if let ArrangementSelection::NoteClip {
                    track_id: selected_track,
                    clip_id,
                } = selection
                {
                    (*selected_track == track_id).then_some(*clip_id)
                } else {
                    None
                }
            })
        })
}

fn focused_note_clip_for_track(editor: &TimelineEditorState, track_id: TrackId) -> Option<ClipId> {
    editor
        .selected_note_clip
        .filter(|(selected_track, _)| *selected_track == track_id)
        .map(|(_, clip_id)| clip_id)
}

fn single_selected_audio_clip_for_track(
    editor: &TimelineEditorState,
    track_id: TrackId,
) -> Option<ClipId> {
    let mut selections = editor.selected_clips.iter();
    match (selections.next(), selections.next()) {
        (
            Some(ArrangementSelection::AudioClip {
                track_id: selected_track,
                clip_id,
            }),
            None,
        ) if *selected_track == track_id => Some(*clip_id),
        _ => None,
    }
}

impl App {
    // ── Detail panel (Ableton-style device chain) ──

    /// The selected MIDI track that can host the piano-roll detail view.
    ///
    /// Visibility and keyboard focus intentionally share this structural
    /// gate, then resolve their clip from different selection state.
    fn piano_roll_track(&self) -> Option<TrackId> {
        if self.state.view.detail_panel_tab != DetailPanelTab::Clip {
            return None;
        }

        let editor = self.state.active_timeline_editor();
        let track_id = editor.selected_track?;
        self.state
            .find_track(track_id)
            .is_some_and(|track| track.kind.is_midi())
            .then_some(track_id)
    }

    /// A note clip the piano roll can render. Arrangement selection may
    /// supply the clip without transferring keyboard focus into the editor.
    pub(super) fn visible_piano_roll_clip(&self) -> Option<(TrackId, ClipId)> {
        let track_id = self.piano_roll_track()?;
        let clip_id = visible_note_clip_for_track(self.state.active_timeline_editor(), track_id)?;
        Some((track_id, clip_id))
    }

    /// The explicitly opened note clip that owns editor shortcuts.
    pub(super) fn focused_piano_roll_clip(&self) -> Option<(TrackId, ClipId)> {
        let track_id = self.piano_roll_track()?;
        let clip_id = focused_note_clip_for_track(self.state.active_timeline_editor(), track_id)?;
        Some((track_id, clip_id))
    }

    /// The clip whose open editor owns the editor shortcuts: an opened
    /// note clip, or the one audio clip the audio editor shows.
    pub(super) fn focused_clip_editor(&self) -> Option<ClipId> {
        self.focused_piano_roll_clip()
            .map(|(_, clip_id)| clip_id)
            .or_else(|| {
                if self.state.view.detail_panel_tab != DetailPanelTab::Clip {
                    return None;
                }
                let editor = self.state.active_timeline_editor();
                single_selected_audio_clip_for_track(editor, editor.selected_track?)
            })
    }

    /// Whether the open piano roll has notes selected: the arrow keys then
    /// move them, ahead of anything else that wants the arrows.
    pub(super) fn piano_roll_has_selected_notes(&self) -> bool {
        self.visible_piano_roll_clip()
            .and_then(|(track_id, clip_id)| {
                let content = self.state.active_timeline_content(track_id)?;
                content
                    .note_clips
                    .iter()
                    .find(|clip| clip.id == clip_id)
                    .map(|clip| !clip.selected_notes.is_empty())
            })
            .unwrap_or(false)
    }

    /// The open clip editor's step on the clip editor grid; quantize uses
    /// it so it matches the lines the editor draws.
    pub(super) fn clip_editor_grid(&self) -> crate::state::SnapGrid {
        self.state
            .view
            .clip_grid
            .effective_grid(self.clip_editor_pixels_per_beat())
    }

    /// The open clip editor's zoom, measured against the window's width
    /// until the producer zooms it; with no clip open the timeline's zoom
    /// stands in.
    pub(super) fn clip_editor_pixels_per_beat(&self) -> f32 {
        self.open_clip_editor_view()
            .map(|view| {
                crate::widgets::piano_roll::view_geometry(view, self.state.view.window_width)
                    .pixels_per_beat()
            })
            .unwrap_or_else(|| self.timeline_pixels_per_beat())
    }

    fn open_clip_editor_view(&self) -> Option<ClipEditorView> {
        let editor = self.state.active_timeline_editor();
        if let Some((track_id, clip_id)) = editor.selected_note_clip {
            let clip = self
                .state
                .active_timeline_content(track_id)?
                .note_clips
                .iter()
                .find(|clip| clip.id == clip_id)?;
            return Some(crate::widgets::piano_roll::framed_view(
                self.state.view.clip_editor_view(clip.id),
                clip.loop_enabled
                    .then_some((clip.loop_start_beats, clip.loop_end_beats)),
                clip.editor_beats(),
            ));
        }
        let track_id = editor.selected_track?;
        let clip_id = single_selected_audio_clip_for_track(editor, track_id)?;
        let clip = self
            .state
            .active_timeline_content(track_id)?
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)?;
        Some(audio_clip_editor_view(
            self.state.view.clip_editor_view(clip.id),
            self.state.transport.samples_per_beat(),
            clip.editor_frames(),
            clip.timeline(),
        ))
    }

    /// The Snap list for the clip editor grid, shown in each clip editor.
    pub(super) fn clip_grid_snap_list(&self) -> Element<'_, Message> {
        let grid = self.state.view.clip_grid;
        let pixels_per_beat = self.clip_editor_pixels_per_beat();
        let snap_picker = pick_list(
            grid.choices(pixels_per_beat),
            Some(grid.choice(pixels_per_beat)),
            |choice| Message::View(ViewMsg::Grid(GridTarget::Clip, GridCommand::Choose(choice))),
        )
        .width(Length::Fixed(90.0))
        .padding([3, 8])
        .text_size(11);
        let snap_label = text("Snap:").size(10).color(th::text_dim());
        row![snap_label, snap_picker]
            .spacing(4)
            .align_y(iced::Alignment::Center)
            .into()
    }

    fn midi_clip_editor_visible(&self) -> bool {
        self.visible_piano_roll_clip().is_some()
    }

    fn detail_editor_preferred_height(&self) -> f32 {
        if self.midi_clip_editor_visible() {
            return MIDI_DETAIL_PANEL_PREFERRED_HEIGHT;
        }
        let editor = self.state.active_timeline_editor();
        let audio_selected = editor.selected_track.is_some_and(|track_id| {
            single_selected_audio_clip_for_track(editor, track_id).is_some()
        });
        if audio_selected {
            AUDIO_DETAIL_PANEL_PREFERRED_HEIGHT
        } else {
            DETAIL_PANEL_MIN_HEIGHT
        }
    }

    pub(super) fn view_detail_panel(&self) -> Element<'_, Message> {
        let detail_content: Element<'_, Message> = if let Some(track) = self
            .state
            .active_timeline_editor()
            .selected_track
            .and_then(|id| self.state.find_track(id))
        {
            let track_id = track.id;
            let track_color = th::track_color(track.color_index);

            let tab = self.state.view.detail_panel_tab;
            let tabs = [
                ("Clip", DetailPanelTab::Clip),
                ("Devices", DetailPanelTab::Devices),
            ]
            .map(|(label, choice)| crate::widgets::segment_group::Choice {
                label,
                shortcut: None,
                selected: tab == choice,
                on_press: Message::View(ViewMsg::SwitchDetailTab(choice)),
            });
            let tab_bar = row![
                crate::widgets::segment_group::switch(tabs),
                text(&track.name)
                    .shaping(crate::typography::USER_TEXT_SHAPING)
                    .size(12)
                    .font(crate::typography::TRACK_TITLE)
                    .color(th::text()),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);

            // Tab content
            let tab_content: Element<'_, Message> = match self.state.view.detail_panel_tab {
                DetailPanelTab::Clip => {
                    let is_midi = track.kind.is_midi();

                    if self.visible_piano_roll_clip().is_some() {
                        self.view_piano_roll_panel(track_id, track_color)
                    } else if is_midi {
                        self.view_midi_track_clip_placeholder(track_id, track_color)
                    } else {
                        // Find a single selected audio clip on this track
                        let audio_sel = single_selected_audio_clip_for_track(
                            self.state.active_timeline_editor(),
                            track_id,
                        );
                        if let Some(sel_cid) = audio_sel {
                            if let Some(clip) = self
                                .state
                                .active_timeline_content(track_id)
                                .and_then(|content| content.clips.iter().find(|c| c.id == sel_cid))
                            {
                                self.view_audio_clip_panel(track_id, clip, track_color)
                            } else {
                                self.view_clip_placeholder()
                            }
                        } else {
                            self.view_clip_placeholder()
                        }
                    }
                }
                DetailPanelTab::Devices => self.view_device_chain(track_id, track, track_color),
            };

            column![tab_bar, tab_content]
                .spacing(DETAIL_CARD_PADDING)
                .height(Length::Fill)
                .into()
        } else {
            let label = text("Select a track to view devices")
                .size(14)
                .color(th::text_dim());
            center(label)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };

        let height = if self.state.view.workspace == crate::state::Workspace::Mix {
            Length::Fill
        } else {
            Length::Fixed(self.effective_detail_height())
        };
        crate::widgets::shell_card::shell_card(
            container(detail_content).padding(DETAIL_CARD_PADDING),
        )
        .height(height)
        .into()
    }

    pub(super) fn detail_playhead_samples(&self) -> Option<u64> {
        if self.state.view.workspace == crate::state::Workspace::Perform
            && self.state.perform.layout == vibez_project::PerformLayout::Clips
        {
            let id = self.state.perform.clip_editor.selected?;
            return self.state.perform.clip_editor.playhead_samples(
                id,
                self.state.perform.performance_position_samples,
                self.state.transport.samples_per_beat(),
            );
        }
        resolved_detail_playhead_samples(
            self.state.view.workspace == crate::state::Workspace::Perform,
            self.state.perform.selected_section,
            self.state.perform.playing_section,
            self.state.transport.position_samples,
            self.state.perform.section_playhead_samples,
        )
    }

    /// The least height the workspace card keeps: one visible track in
    /// Arrange, the pad surface's room in Perform.
    fn workspace_card_min_height(&self) -> f32 {
        if self.state.view.workspace == crate::state::Workspace::Arrange {
            super::views_arrangement::ARRANGE_ONE_TRACK_VISIBLE_HEIGHT
        } else {
            WORKSPACE_MIN_HEIGHT
        }
    }

    pub(super) fn effective_detail_height(&self) -> f32 {
        effective_detail_panel_height(
            self.state.view.detail_panel_height,
            self.state.view.window_height,
            self.workspace_card_min_height(),
            self.detail_editor_preferred_height(),
        )
    }

    /// The height a drag on the divider asks for: the pointer's distance
    /// from the window's foot, clamped to the card's whole range. The open
    /// editor's preferred height does not pin a drag, so the rack/piano
    /// roll can be enlarged until the workspace is at its minimum — one
    /// visible track in Arrange — and shrunk below the editor's preference.
    pub(super) fn detail_panel_drag_height(&self, cursor_y: f32) -> f32 {
        crate::shell_layout::detail_height_at(cursor_y, self.state.view.window_height).clamp(
            DETAIL_PANEL_MIN_HEIGHT,
            detail_panel_maximum_height(
                self.state.view.window_height,
                self.workspace_card_min_height(),
            ),
        )
    }

    pub(super) fn view_clip_placeholder(&self) -> Element<'_, Message> {
        let label = text("Select a clip to view details")
            .size(14)
            .color(th::text_dim());
        center(label)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The names of a Drum Rack track's pads for the piano roll's keys: the
    /// pad's own name, else its sample's file name, else its pad number.
    /// Empty for any other track.
    fn drum_pad_labels(&self, track_id: TrackId) -> Vec<String> {
        let Some(track) = self.state.find_track(track_id) else {
            return Vec::new();
        };
        if !crate::domains::sampler::is_native_drum_rack(track) {
            return Vec::new();
        }
        track
            .drum_rack_pads
            .iter()
            .enumerate()
            .map(|(index, pad)| {
                pad.name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .or_else(|| {
                        pad.source.as_ref().map(|source| {
                            let name = source.display_name();
                            std::path::Path::new(&name)
                                .file_stem()
                                .map_or(name.clone(), |stem| stem.to_string_lossy().into_owned())
                        })
                    })
                    .unwrap_or_else(|| format!("Pad {}", index + 1))
            })
            .collect()
    }

    /// Piano roll panel for the detail panel split view.
    pub(super) fn view_piano_roll_panel(
        &self,
        track_id: TrackId,
        track_color: Color,
    ) -> Element<'_, Message> {
        use crate::state::PianoRollEditMode;

        let playhead_beats = self
            .detail_playhead_samples()
            .map(|samples| {
                samples as f64 * self.state.transport.bpm
                    / (f64::from(self.state.transport.sample_rate.max(1)) * 60.0)
            })
            .unwrap_or(-1.0);

        let visible_clip = self
            .visible_piano_roll_clip()
            .filter(|(open_track_id, _)| *open_track_id == track_id)
            .and_then(|(_, clip_id)| {
                let content = self.state.active_timeline_content(track_id)?;
                content.note_clips.iter().find(|clip| clip.id == clip_id)
            });

        let (mut piano_widget, mut velocity_widget) = match visible_clip {
            Some(clip) => {
                let content_playhead = clip.content_beat_at(playhead_beats).unwrap_or(-1.0);
                (
                    PianoRollWidget::from_clip(
                        track_id,
                        clip,
                        content_playhead,
                        clip.editor_beats(),
                        track_color,
                        self.state.view.clip_grid,
                        self.state.piano_roll.scroll_y,
                        self.state.piano_roll.edit_mode,
                    ),
                    VelocityLaneWidget::from_clip(
                        track_id,
                        clip,
                        clip.editor_beats(),
                        track_color,
                        self.state.view.clip_grid,
                    ),
                )
            }
            None => (
                PianoRollWidget::empty(track_id, playhead_beats, track_color),
                VelocityLaneWidget::empty(track_id, track_color),
            ),
        };

        let clip_view = visible_clip.and_then(|clip| self.state.view.clip_editor_view(clip.id));
        piano_widget.view = clip_view;
        piano_widget.pad_labels = self.drum_pad_labels(track_id);
        piano_widget.wheel_zoom = self.wheel_zoom();
        velocity_widget.view = clip_view;
        piano_widget.reserve_arrow_keys = self.state.view.workspace
            == crate::state::Workspace::Perform
            && self.state.perform.layout == vibez_project::PerformLayout::Clips
            && self.state.perform.mode == crate::domains::perform::PerformMode::Sections
            && !self.piano_roll_has_selected_notes();
        let piano_canvas: Element<'_, Message> = canvas(piano_widget)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let velocity_canvas: Element<'_, Message> = canvas(velocity_widget)
            .width(Length::Fill)
            .height(Length::Fixed(VelocityLaneWidget::HEIGHT))
            .into();

        // ── Clip properties bar (shown when a clip is selected) ──
        let mut content_col = column![].spacing(2).padding(4);

        if let Some(clip) = visible_clip {
            let clip_id = clip.id;
            let clip_loop = clip.loop_enabled;
            let groove_grid = clip.groove_grid;
            let clip_name = text(clip.name.clone())
                .shaping(crate::typography::USER_TEXT_SHAPING)
                .size(11)
                .color(th::text());
            let pos_label = text(format!("Pos: {:.1}", clip.position_beats))
                .size(10)
                .color(th::text_dim());
            let dur_label = text(vibez_core::time::MusicalTime(clip.duration_beats).length_label())
                .size(10)
                .color(th::text_dim());

            let swing_relationship = self.view_clip_swing_relationship(
                track_id,
                track_color,
                Some((clip_id, groove_grid)),
            );

            // Loop toggle
            let loop_icon_color = if clip_loop {
                th::accent()
            } else {
                th::text_dim()
            };
            let loop_btn = button(icons::icon(icons::REPEAT).size(10).color(loop_icon_color))
                .on_press(Message::PianoRoll(PianoRollMsg::ToggleNoteClipLoop(
                    track_id, clip_id,
                )))
                .padding([2, 4])
                .style(move |_theme: &Theme, _status| button::Style {
                    background: if clip_loop {
                        Some(th::accent_dim().into())
                    } else {
                        Some(th::bg_elevated().into())
                    },
                    text_color: loop_icon_color,
                    border: iced::Border {
                        color: if clip_loop {
                            th::accent_dim()
                        } else {
                            th::border()
                        },
                        width: 1.0,
                        radius: 3.0.into(),
                    },
                    ..Default::default()
                });

            // Clip operation buttons
            let op_btn_style = |_theme: &Theme, _status| button::Style {
                background: Some(th::bg_elevated().into()),
                text_color: th::text_dim(),
                border: iced::Border {
                    color: th::border(),
                    width: 1.0,
                    radius: 3.0.into(),
                },
                ..Default::default()
            };

            let dup_btn = button(
                row![
                    icons::icon(icons::COPY).size(10).color(th::text_dim()),
                    text("Dup").size(10).color(th::text_dim())
                ]
                .spacing(2)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::duplicate_note_clip(track_id, clip_id))
            .padding([2, 6])
            .style(op_btn_style);

            let double_btn = button(text("2x").size(10).color(th::text_dim()))
                .on_press(Message::PianoRoll(PianoRollMsg::DoubleNoteClip(
                    track_id, clip_id,
                )))
                .padding([2, 6])
                .style(op_btn_style);

            let halve_btn = button(text("\u{00BD}x").size(10).color(th::text_dim()))
                .on_press(Message::PianoRoll(PianoRollMsg::HalveNoteClip(
                    track_id, clip_id,
                )))
                .padding([2, 6])
                .style(op_btn_style);

            let crop_btn = button(
                row![
                    icons::icon(icons::SCISSORS).size(10).color(th::text_dim()),
                    text("Crop").size(10).color(th::text_dim())
                ]
                .spacing(2)
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::PianoRoll(PianoRollMsg::CropNoteClip(
                track_id, clip_id,
            )))
            .padding([2, 6])
            .style(op_btn_style);

            let props_row = row![
                clip_name,
                swing_relationship,
                horizontal_space(),
                pos_label,
                dur_label,
                loop_btn,
                dup_btn,
                double_btn,
                halve_btn,
                crop_btn,
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);

            content_col = content_col.push(props_row);
        }

        // ── Header row: label, edit mode toggle, snap grid ──
        let label = text("Piano Roll").size(11).color(th::text_dim());

        // Edit mode toggle: Select / Draw
        let select_active = self.state.piano_roll.edit_mode == PianoRollEditMode::Select;
        let draw_active = self.state.piano_roll.edit_mode == PianoRollEditMode::Draw;

        let select_btn = {
            let (bg, tc) = if select_active {
                (th::accent_dim(), th::accent())
            } else {
                (th::bg_elevated(), th::text_dim())
            };
            button(icons::icon(icons::MOUSE_POINTER).size(10).color(tc))
                .on_press(Message::PianoRoll(PianoRollMsg::ToggleEditMode))
                .padding([2, 5])
                .style(move |_theme: &Theme, _status| button::Style {
                    background: Some(bg.into()),
                    text_color: tc,
                    border: iced::Border {
                        color: if select_active {
                            th::accent_dim()
                        } else {
                            th::border()
                        },
                        width: 1.0,
                        radius: 3.0.into(),
                    },
                    ..Default::default()
                })
        };

        let draw_btn = {
            let (bg, tc) = if draw_active {
                (th::accent_dim(), th::accent())
            } else {
                (th::bg_elevated(), th::text_dim())
            };
            button(icons::icon(icons::PENCIL).size(10).color(tc))
                .on_press(Message::PianoRoll(PianoRollMsg::ToggleEditMode))
                .padding([2, 5])
                .style(move |_theme: &Theme, _status| button::Style {
                    background: Some(bg.into()),
                    text_color: tc,
                    border: iced::Border {
                        color: if draw_active {
                            th::accent_dim()
                        } else {
                            th::border()
                        },
                        width: 1.0,
                        radius: 3.0.into(),
                    },
                    ..Default::default()
                })
        };

        let mode_row = row![select_btn, draw_btn].spacing(1);

        let header_row = row![
            label,
            mode_row,
            horizontal_space(),
            self.clip_grid_snap_list()
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);

        content_col = content_col
            .push(header_row)
            .push(piano_canvas)
            .push(velocity_canvas);

        container(content_col.height(Length::Fill))
            .width(Length::FillPortion(1))
            .height(Length::Fill)
            .style(|_theme: &Theme| container::Style {
                background: Some(th::bg_dark().into()),
                border: iced::Border {
                    color: th::border(),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        effective_detail_panel_height, focused_note_clip_for_track,
        resolved_detail_playhead_samples, single_selected_audio_clip_for_track,
        visible_note_clip_for_track,
    };
    use crate::state::{ArrangementSelection, TimelineEditorState};
    use vibez_core::id::{ClipId, SectionId, TrackId};

    #[test]
    fn detail_panel_height_preserves_the_workspace_at_small_windows() {
        // A dragged height is clamped to the window's range, so it never
        // crushes the workspace card into a letterbox.
        assert_eq!(
            effective_detail_panel_height(Some(80.0), 900.0, 440.0, 180.0),
            180.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(280.0), 900.0, 440.0, 180.0),
            280.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(320.0), 520.0, 440.0, 180.0),
            180.0
        );
    }

    #[test]
    fn a_dragged_height_can_enlarge_the_rack_past_the_resting_composition() {
        // With the 440 px workspace minimum, the rack may grow to all the
        // room the window leaves below it: 900 - 94 - 440 = 366 px, past
        // the 35% resting cap of 315 px.
        assert_eq!(
            effective_detail_panel_height(Some(800.0), 900.0, 440.0, 180.0),
            366.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(800.0), 1_000.0, 440.0, 180.0),
            466.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(500.0), 1_200.0, 440.0, 180.0),
            500.0
        );
    }

    #[test]
    fn in_arrange_the_rack_grows_until_one_track_stays_visible() {
        // The Arrange workspace only needs its overview, ruler, one
        // minimum lane, the master row and the time ruler: 34 + 36 + 62 +
        // 26 + 20 = 178 px. A 900 px window then leaves 900 - 94 - 178 =
        // 628 px for the rack.
        let one_track = super::super::views_arrangement::ARRANGE_ONE_TRACK_VISIBLE_HEIGHT;
        assert_eq!(one_track, 178.0);
        assert_eq!(
            effective_detail_panel_height(Some(800.0), 900.0, one_track, 180.0),
            628.0
        );
        // The resting height still keeps the 35% composition: the 280 px
        // default fits under the 315 px cap.
        assert_eq!(
            effective_detail_panel_height(None, 900.0, one_track, 180.0),
            280.0
        );
        assert_eq!(
            effective_detail_panel_height(None, 900.0, one_track, 360.0),
            315.0
        );
    }

    #[test]
    fn detail_panel_cannot_recreate_the_crushed_perform_layout() {
        // Even at the drag maximum, a 1,032 px window keeps its chrome,
        // status bar, gaps and 440 px of workspace: 1,032 - 94 - 440 = 498 px.
        let height = effective_detail_panel_height(Some(900.0), 1_032.0, 440.0, 180.0);
        assert!((height - 498.0).abs() < 0.001);
    }

    #[test]
    fn until_dragged_the_panel_rests_at_the_default_grown_to_the_editor_s_preference() {
        assert_eq!(
            effective_detail_panel_height(None, 900.0, 440.0, 180.0),
            280.0
        );
        // The MIDI editor's 360 px preference yields to the window's bound.
        assert_eq!(
            effective_detail_panel_height(None, 900.0, 440.0, 360.0),
            315.0
        );
        assert_eq!(
            effective_detail_panel_height(None, 520.0, 440.0, 360.0),
            180.0
        );
        // 52 px chrome row, 26 px status bar and two 8 px gaps leave 440 px
        // for the workspace of a 760 px window above a 226 px detail card.
        assert_eq!(
            effective_detail_panel_height(None, 760.0, 440.0, 360.0),
            226.0
        );
        // The audio inspector's 260 px does not shrink the 280 px default.
        assert_eq!(
            effective_detail_panel_height(None, 900.0, 440.0, 260.0),
            280.0
        );
    }

    #[test]
    fn a_dragged_height_wins_over_the_open_editor_s_preference() {
        // The rack/piano-roll panel can be resized over its whole range even
        // while the piano roll (360 px preferred) or the audio inspector
        // (260 px) is open: the drag is no longer pinned from below.
        assert_eq!(
            effective_detail_panel_height(Some(200.0), 900.0, 440.0, 360.0),
            200.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(280.0), 900.0, 440.0, 360.0),
            280.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(420.0), 900.0, 440.0, 360.0),
            366.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(180.0), 900.0, 440.0, 260.0),
            180.0
        );
        assert_eq!(
            effective_detail_panel_height(Some(320.0), 900.0, 440.0, 260.0),
            320.0
        );
    }

    #[test]
    fn detail_playhead_resolves_arrange_and_section_clocks_without_crossing_targets() {
        let playing = SectionId::new();
        let other = SectionId::new();

        assert_eq!(
            resolved_detail_playhead_samples(false, None, None, 96_000, 12_000),
            Some(96_000)
        );
        assert_eq!(
            resolved_detail_playhead_samples(true, Some(playing), Some(playing), 96_000, 12_000,),
            Some(12_000)
        );
        assert_eq!(
            resolved_detail_playhead_samples(true, Some(other), Some(playing), 96_000, 12_000,),
            None
        );
    }

    #[test]
    fn audio_inspector_never_chooses_an_arbitrary_clip_from_multi_selection() {
        let track_id = TrackId::new();
        let first = ClipId::new();
        let second = ClipId::new();
        let mut editor = TimelineEditorState::default();
        editor
            .selected_clips
            .insert(ArrangementSelection::AudioClip {
                track_id,
                clip_id: first,
            });
        assert_eq!(
            single_selected_audio_clip_for_track(&editor, track_id),
            Some(first)
        );
        editor
            .selected_clips
            .insert(ArrangementSelection::AudioClip {
                track_id,
                clip_id: second,
            });
        assert_eq!(
            single_selected_audio_clip_for_track(&editor, track_id),
            None
        );
    }

    #[test]
    fn marquee_selected_note_clip_is_the_visible_piano_roll_clip() {
        let track_id = TrackId::new();
        let other_track = TrackId::new();
        let marquee_clip = ClipId::new();
        let explicit_clip = ClipId::new();
        let mut editor = TimelineEditorState::default();
        editor
            .selected_clips
            .insert(ArrangementSelection::NoteClip {
                track_id,
                clip_id: marquee_clip,
            });

        assert_eq!(
            visible_note_clip_for_track(&editor, track_id),
            Some(marquee_clip)
        );
        assert_eq!(visible_note_clip_for_track(&editor, other_track), None);

        editor.selected_note_clip = Some((track_id, explicit_clip));
        assert_eq!(
            visible_note_clip_for_track(&editor, track_id),
            Some(explicit_clip)
        );
    }

    #[test]
    fn arrangement_selection_does_not_focus_the_piano_roll_for_select_all() {
        let track_id = TrackId::new();
        let selected_clip = ClipId::new();
        let explicitly_open_clip = ClipId::new();
        let mut editor = TimelineEditorState::default();
        editor
            .selected_clips
            .insert(ArrangementSelection::NoteClip {
                track_id,
                clip_id: selected_clip,
            });

        // This is the state after the first arrangement Command+A. A
        // second press must remain arrangement select-all.
        assert_eq!(focused_note_clip_for_track(&editor, track_id), None);

        editor.selected_note_clip = Some((track_id, explicitly_open_clip));
        assert_eq!(
            focused_note_clip_for_track(&editor, track_id),
            Some(explicitly_open_clip)
        );
    }
}

#[cfg(test)]
mod clip_editor_zoom_tests {
    use std::sync::Arc;

    use vibez_core::id::TrackId;

    use crate::state::{ProjectTrack, SnapGrid, UiNoteClip};

    #[test]
    fn a_trimmed_note_clip_snaps_at_the_zoom_its_piano_roll_shows() {
        let mut app = crate::app::test_support::app();
        app.state.view.window_width = 1_052.0;
        let mut track = ProjectTrack::new(TrackId::new(), "MIDI".into(), 0);
        track.kind = vibez_core::midi::TrackKind::Midi;
        let track_id = track.id;
        Arc::make_mut(&mut app.state.project_tracks)
            .tracks
            .push(track);
        // The piano roll shows its content from beat 0 to beat 32, where
        // this Clip stops playing.
        let clip = UiNoteClip {
            name: "Trimmed".into(),
            start_marker_beats: 28.0,
            loop_end_beats: 32.0,
            ..UiNoteClip::test_clip(0.0, 4.0)
        };
        let clip_id = clip.id;
        app.state
            .arrange_content_mut(track_id)
            .note_clips
            .push(clip);
        app.state.arrangement.selected_track = Some(track_id);
        app.state.arrangement.selected_note_clip = Some((track_id, clip_id));

        assert_eq!(app.clip_editor_grid(), SnapGrid::EIGHTH);
    }
}
