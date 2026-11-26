//! The Modulators card, at the end of a channel's device chain: one
//! modulator's settings beside the parameters it moves.

use iced::widget::{button, canvas, column, container, row, text, Space};
use iced::{Alignment, Element, Length};
use vibez_core::modulation::{
    can_modulate, ModDivision, ModShape, Modulator, ModulatorKind, ModulatorSettings,
    MAX_MODULATION_LINKS, MAX_MODULATORS,
};

use crate::domains::modulation::{ModField, ModulationMsg};
use crate::icons;
use crate::message::Message;
use crate::state::UiTrack;
use crate::theme as th;
use crate::typography;
use crate::widgets::device_chrome::{shell, Header};
use crate::widgets::device_controls::{dropdown, segmented, toggle, vertical_rule};
use crate::widgets::effect_knob::{EffectKnobWidget, KnobTarget};
use crate::widgets::modulator_display::LfoPlot;

use super::views_automation::LaneChoice;
use super::*;

/// Width of the card with no modulator, and with some.
pub(super) const MODULATOR_EMPTY_W: f32 = 180.0;
pub(super) const MODULATOR_CARD_W: f32 = 520.0;
const SETTINGS_W: f32 = 232.0;
const KNOB: f32 = 30.0;
const KNOB_COL_W: f32 = 54.0;
const LINK_KNOB: f32 = 18.0;
const LINK_ROW_H: f32 = 20.0;

/// A menu entry naming a [`ModShape`] or [`ModDivision`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct Named<T>(T);

impl std::fmt::Display for Named<ModShape> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.label())
    }
}

impl std::fmt::Display for Named<ModDivision> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.label())
    }
}

fn field_text(field: ModField, value: f32) -> String {
    match field {
        ModField::Rate => format!("{value:.2} Hz"),
        ModField::Phase => format!("{:.0} deg", value * 360.0),
        ModField::Smooth | ModField::Sustain => format!("{:.0} %", value * 100.0),
        ModField::Amount(_) => format!("{:+.0} %", value * 100.0),
        ModField::Attack | ModField::Decay | ModField::Release => {
            if value >= 1_000.0 {
                format!("{:.2} s", value / 1_000.0)
            } else {
                format!("{value:.0} ms")
            }
        }
    }
}

impl App {
    pub(super) fn modulator_card_width(&self, track: &UiTrack) -> f32 {
        if track.modulators.is_empty() {
            MODULATOR_EMPTY_W
        } else {
            MODULATOR_CARD_W
        }
    }

    pub(super) fn view_modulator_card<'a>(
        &'a self,
        track: &'a UiTrack,
        accent: iced::Color,
    ) -> Element<'a, Message> {
        let id = track.id;
        let count = track.modulators.len();
        let selected = self.state.modulation_ui.selected(id, count);
        let message = move |msg: ModulationMsg| Message::Modulation(msg);

        let mut add = row![].spacing(2);
        if count < MAX_MODULATORS {
            for kind in ModulatorKind::ALL {
                add = add.push(
                    button(text(format!("+ {}", kind.label())).size(9))
                        .on_press(message(ModulationMsg::Add(id, kind)))
                        .padding([2, 4])
                        .style(crate::widgets::control_style::style(None)),
                );
            }
        }
        let header = Header::new(
            "Modulators",
            accent,
            "Move this channel's parameters".into(),
        );
        let width = self.modulator_card_width(track);
        let Some(modulator) = track.modulators.get(selected) else {
            let body = column![
                text("Add a modulator, then choose what it moves.")
                    .size(10)
                    .color(th::text_muted()),
                add.wrap(),
            ]
            .spacing(8)
            .padding(10);
            return shell(header, body, width);
        };

        let names: Vec<String> = track
            .modulators
            .iter()
            .enumerate()
            .map(|(index, modulator)| format!("{} {}", index + 1, modulator.settings.kind.label()))
            .collect();
        let mut tabs = row![].spacing(2);
        for (index, name) in names.into_iter().enumerate() {
            tabs = tabs.push(toggle(
                text(name).size(9).into(),
                index == selected,
                message(ModulationMsg::Select(id, index)),
                accent,
            ));
        }

        let settings = self.modulator_settings(track, selected, modulator, accent);
        let links = self.modulator_links(track, selected, modulator, accent);
        let current = modulator.settings;
        let switch = segmented(
            &[(true, "On"), (false, "Off")],
            current.enabled,
            move |enabled| {
                message(ModulationMsg::SetSettings(
                    id,
                    selected,
                    ModulatorSettings { enabled, ..current },
                ))
            },
            accent,
        );
        let remove = crate::widgets::hint::with_hint(
            button(icons::icon(icons::TRASH_2).size(12).color(th::text_dim()))
                .on_press(message(ModulationMsg::Remove(id, selected)))
                .padding([2, 6])
                .style(crate::widgets::control_style::style(None)),
            "Remove this modulator",
        );
        let bar = row![tabs, switch, remove, Space::with_width(Length::Fill), add]
            .spacing(8)
            .align_y(Alignment::Center);
        let body = column![
            container(bar).padding([4, 10]),
            row![
                container(settings)
                    .width(Length::Fixed(SETTINGS_W))
                    .padding([0, 10]),
                vertical_rule(),
                container(links).width(Length::Fill).padding([0, 10]),
            ]
            .height(Length::Fill),
        ];
        shell(header, body, width)
    }

    fn modulator_settings<'a>(
        &'a self,
        track: &'a UiTrack,
        index: usize,
        modulator: &'a Modulator,
        accent: iced::Color,
    ) -> Element<'a, Message> {
        let id = track.id;
        let settings = modulator.settings;
        let set = move |settings: ModulatorSettings| {
            Message::Modulation(ModulationMsg::SetSettings(id, index, settings))
        };
        let shape = dropdown(
            ModShape::ALL.into_iter().map(Named).collect(),
            Named(settings.shape),
            move |choice: Named<ModShape>| {
                set(ModulatorSettings {
                    shape: choice.0,
                    ..settings
                })
            },
        );
        let division = dropdown(
            ModDivision::ALL.into_iter().map(Named).collect(),
            Named(settings.division),
            move |choice: Named<ModDivision>| {
                set(ModulatorSettings {
                    division: choice.0,
                    ..settings
                })
            },
        );
        let free = settings.division == ModDivision::Free;
        let (menus, fields): (Element<'a, Message>, Vec<(ModField, &str)>) = match settings.kind {
            ModulatorKind::Lfo => (row![shape, division].spacing(4).into(), {
                let mut fields = Vec::new();
                if free {
                    fields.push((ModField::Rate, "Rate"));
                }
                fields.push((ModField::Phase, "Phase"));
                fields
            }),
            ModulatorKind::Random => (division, {
                let mut fields = Vec::new();
                if free {
                    fields.push((ModField::Rate, "Rate"));
                }
                fields.push((ModField::Smooth, "Smooth"));
                fields
            }),
            ModulatorKind::Envelope => (
                text("Starts with the track's notes")
                    .size(9)
                    .color(th::text_muted())
                    .into(),
                vec![
                    (ModField::Attack, "Attack"),
                    (ModField::Decay, "Decay"),
                    (ModField::Sustain, "Sustain"),
                    (ModField::Release, "Release"),
                ],
            ),
        };

        let mut knobs = row![];
        for (field, name) in fields {
            let value = field.read(modulator);
            knobs = knobs.push(
                column![
                    text(name).size(10).color(th::text_dim()),
                    self.modulator_knob(track, index, field, value, KNOB, accent),
                    text(field_text(field, value))
                        .font(typography::PERFORM_TECH)
                        .size(10)
                        .color(th::text()),
                ]
                .width(Length::Fixed(KNOB_COL_W))
                .align_x(Alignment::Center),
            );
        }
        let mut column = column![menus].spacing(4);
        if settings.kind == ModulatorKind::Lfo {
            // The curve takes what height is left, and goes first.
            column = column.push(
                canvas(LfoPlot {
                    settings,
                    cycles: self.modulator_cycles(&settings),
                    accent,
                })
                .width(Length::Fill)
                .height(Length::Fill),
            );
        }
        column.push(knobs).into()
    }

    /// Cycles a modulator has run, on the clock the engine reads: the song
    /// position while playing, and otherwise time passing.
    fn modulator_cycles(&self, settings: &ModulatorSettings) -> f64 {
        let transport = &self.state.transport;
        let (beat, seconds) = if transport.is_playing() {
            (
                transport.position_beats(),
                transport.position_samples as f64 / f64::from(transport.sample_rate.max(1)),
            )
        } else {
            let seconds = self.state.modulation_ui.seconds_running();
            (seconds * transport.bpm / 60.0, seconds)
        };
        settings.cycles(beat, seconds) - f64::from(settings.phase)
    }

    /// Whether a moving LFO plot is on screen, which needs a fast tick.
    pub(super) fn lfo_plot_shown(&self) -> bool {
        if self.state.view.detail_panel_tab != crate::state::DetailPanelTab::Devices {
            return false;
        }
        let Some(track) = self
            .state
            .arrangement
            .selected_track
            .and_then(|id| self.state.find_track(id))
        else {
            return false;
        };
        let count = track.modulators.len();
        let selected = self.state.modulation_ui.selected(track.id, count);
        track.modulators.get(selected).is_some_and(|modulator| {
            modulator.settings.enabled && modulator.settings.kind == ModulatorKind::Lfo
        })
    }

    fn modulator_knob<'a>(
        &'a self,
        track: &'a UiTrack,
        index: usize,
        field: ModField,
        value: f32,
        size: f32,
        accent: iced::Color,
    ) -> Element<'a, Message> {
        let (min, max, default) = field.range();
        // A curved control turns in knob position, 0..1.
        let (value, min, max, default) = if field.is_curved() {
            (
                field.position_of(value),
                0.0,
                1.0,
                field.position_of(default),
            )
        } else {
            (value, min, max, default)
        };
        let mut knob =
            EffectKnobWidget::for_instrument(track.id, 0, value, min, max, default, accent);
        knob.target = KnobTarget::Modulator {
            modulator: index,
            field,
        };
        knob.bipolar = matches!(field, ModField::Amount(_));
        canvas(knob)
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .into()
    }

    /// The parameters the modulator moves, in two columns, and the menu that
    /// adds one.
    fn modulator_links<'a>(
        &'a self,
        track: &'a UiTrack,
        index: usize,
        modulator: &'a Modulator,
        accent: iced::Color,
    ) -> Element<'a, Message> {
        let id = track.id;
        let mut columns = [column![].spacing(2), column![].spacing(2)];
        for (position, link) in modulator.links.iter().enumerate() {
            let label = crate::domains::automation::target_label(&link.target, track);
            let line = row![
                self.modulator_knob(
                    track,
                    index,
                    ModField::Amount(position),
                    link.amount,
                    LINK_KNOB,
                    accent,
                ),
                text(typography::fit(&label, 64.0, 9.0, iced::Font::DEFAULT))
                    .size(9)
                    .color(th::text_dim())
                    .shaping(typography::USER_TEXT_SHAPING)
                    .wrapping(text::Wrapping::None)
                    .width(Length::Fill),
                text(field_text(ModField::Amount(position), link.amount))
                    .font(typography::PERFORM_TECH)
                    .size(9)
                    .color(th::text()),
                crate::widgets::hint::with_hint(
                    button(icons::icon(icons::X).size(10).color(th::text_dim()))
                        .on_press(Message::Modulation(ModulationMsg::RemoveLink(
                            id, index, position,
                        )))
                        .padding([2, 3])
                        .style(crate::widgets::control_style::style(None)),
                    "Stop moving this parameter",
                ),
            ]
            .spacing(4)
            .height(Length::Fixed(LINK_ROW_H))
            .align_y(Alignment::Center);
            let side = position % 2;
            let taken = std::mem::replace(&mut columns[side], column![]);
            columns[side] = taken.push(line);
        }
        let [left, right] = columns;

        let mut content = column![].spacing(4);
        if modulator.links.len() < MAX_MODULATION_LINKS {
            let choices: Vec<LaneChoice> = self
                .automation_lane_choices(track, &[])
                .into_iter()
                .filter(|choice| {
                    can_modulate(choice.target)
                        && !modulator
                            .links
                            .iter()
                            .any(|link| link.target == choice.target)
                        // Plugin instruments do not report their values.
                        && !(track.plugin_instrument_name.is_some()
                            && matches!(
                                choice.target,
                                vibez_core::automation::AutomationTarget::InstrumentParam { .. }
                            ))
                })
                .filter(|choice| match choice.target {
                    vibez_core::automation::AutomationTarget::EffectParam { effect_id, .. } => {
                        track
                            .effects
                            .iter()
                            .any(|effect| effect.id == effect_id && effect.plugin_ref.is_none())
                    }
                    _ => true,
                })
                .collect();
            content = content.push(
                iced::widget::pick_list(choices, None::<LaneChoice>, move |choice| {
                    Message::Modulation(ModulationMsg::AddLink(id, index, choice.target))
                })
                .placeholder("Add a target to move")
                .width(Length::Fill)
                .padding([3, 5])
                .text_size(9)
                .style(crate::widgets::control_style::list(accent)),
            );
        }
        content
            .push(row![left.width(Length::Fill), right.width(Length::Fill)].spacing(8))
            .into()
    }
}
