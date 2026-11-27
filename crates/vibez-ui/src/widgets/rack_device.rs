//! The Effect Rack's card body: macros over the chains, and the controls of
//! the chosen device.

use iced::widget::{button, canvas, column, container, pick_list, row, text, Space};
use iced::{Alignment, Color, Element, Length};
use vibez_core::effect::EffectType;
use vibez_core::id::TrackId;
use vibez_core::rack::{RackState, CHAIN_ON, CHAIN_VOLUME, MACROS, MAX_CHAINS, MAX_CHAIN_DEVICES};

use crate::domains::devices::DevicesMsg;
use crate::domains::rack::{descriptors, device_value, RackEdit, RackMsg};
use crate::icons;
use crate::message::Message;
use crate::state::UiEffect;
use crate::theme as th;
use crate::typography;
use crate::widgets::device_controls::{rule, toggle};
use crate::widgets::effect_knob::{EffectKnobWidget, KnobTarget};
use crate::widgets::{control_style, hint, quiet_scroll};

pub const RACK_CARD_W: f32 = 560.0;
const MACRO_KNOB: f32 = 34.0;
const PARAM_KNOB: f32 = 30.0;
const PARAM_COL_W: f32 = 76.0;
const PARAMS_PER_ROW: usize = 7;

/// A menu entry giving a parameter to a macro, or to none.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MacroChoice(Option<usize>);

impl std::fmt::Display for MacroChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(index) => write!(formatter, "M{}", index + 1),
            None => formatter.write_str("–"),
        }
    }
}

/// A menu entry naming an effect a chain can take.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Insert(EffectType);

impl std::fmt::Display for Insert {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.name())
    }
}

fn value_label(value: f32, unit: &str) -> String {
    let number = if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    };
    if unit.is_empty() {
        number
    } else {
        format!("{number} {unit}")
    }
}

fn small_button<'a>(label: String, lit: Option<Color>, message: Message) -> Element<'a, Message> {
    button(text(label).size(9))
        .on_press(message)
        .padding([2, 5])
        .style(control_style::style(lit))
        .into()
}

/// `shown` is the chain and device the card displays.
pub fn body<'a>(
    track_id: TrackId,
    effect: &'a UiEffect,
    rack: &'a RackState,
    accent: Color,
    shown: (usize, usize),
) -> Element<'a, Message> {
    let accent = if effect.bypass {
        th::text_muted()
    } else {
        accent
    };
    let effect_id = effect.id;
    let message = move |edit: RackEdit| {
        Message::Rack(RackMsg {
            track_id,
            effect_id,
            edit,
        })
    };
    let (chain, device) = shown;
    let knob = |index: usize, min: f32, max: f32, default: f32, value: f32, size: f32| {
        let mut knob =
            EffectKnobWidget::for_instrument(track_id, index, value, min, max, default, accent);
        knob.target = KnobTarget::Effect(effect_id);
        canvas(knob)
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
    };
    let param = |index: usize, default: f32| effect.params.get(index).copied().unwrap_or(default);

    // Macros: the rack's own knobs, automatable and mappable by MIDI.
    let mut macros = row![];
    for index in 0..MACROS {
        let value = param(index, 0.0);
        let used = rack
            .mappings
            .iter()
            .any(|mapping| usize::from(mapping.macro_index) == index);
        macros = macros.push(
            column![
                text(format!("Macro {}", index + 1)).size(9).color(if used {
                    th::text()
                } else {
                    th::text_muted()
                }),
                knob(index, 0.0, 1.0, 0.0, value, MACRO_KNOB),
                text(format!("{:.0} %", value * 100.0))
                    .font(typography::PERFORM_TECH)
                    .size(9)
                    .color(th::text_dim()),
            ]
            .width(Length::FillPortion(1))
            .align_x(Alignment::Center),
        );
    }

    // Chains: one tab each, then the chosen one's level and switch.
    let mut chains = row![].spacing(2).align_y(Alignment::Center);
    for index in 0..rack.chains.len() {
        chains = chains.push(toggle(
            text(format!("Chain {}", index + 1)).size(9).into(),
            index == chain,
            message(RackEdit::Select {
                chain: index,
                device: 0,
            }),
            accent,
        ));
    }
    if rack.chains.len() < MAX_CHAINS {
        chains = chains.push(hint::with_hint(
            small_button("+".into(), None, message(RackEdit::AddChain)),
            "Add a parallel chain",
        ));
    }
    let on = param(CHAIN_ON + chain, 1.0) >= 0.5;
    let volume = param(CHAIN_VOLUME + chain, 1.0);
    let db = if volume > 0.0 {
        format!("{:+.1} dB", 20.0 * volume.log10())
    } else {
        "-inf dB".into()
    };
    chains = chains
        .push(Space::with_width(Length::Fill))
        .push(toggle(
            text(if on { "On" } else { "Off" }).size(9).into(),
            on,
            Message::Devices(DevicesMsg::SetEffectParam(
                track_id,
                effect_id,
                CHAIN_ON + chain,
                if on { 0.0 } else { 1.0 },
            )),
            accent,
        ))
        .push(knob(CHAIN_VOLUME + chain, 0.0, 2.0, 1.0, volume, 26.0))
        .push(
            text(db)
                .font(typography::PERFORM_TECH)
                .size(9)
                .color(th::text_dim())
                .width(Length::Fixed(52.0)),
        );
    if rack.chains.len() > 1 {
        chains = chains.push(hint::with_hint(
            button(icons::icon(icons::TRASH_2).size(11).color(th::text_dim()))
                .on_press(message(RackEdit::RemoveChain(chain)))
                .padding([2, 5])
                .style(control_style::style(None)),
            "Remove this chain",
        ));
    }

    // Devices of the chosen chain, in series.
    let held = rack.chains.get(chain).map_or(&[][..], |c| &c.devices[..]);
    let mut devices = row![].spacing(2).align_y(Alignment::Center);
    for (index, slot) in held.iter().enumerate() {
        devices = devices.push(toggle(
            text(slot.effect_type.name())
                .size(9)
                .color(if slot.bypass {
                    th::text_muted()
                } else if index == device {
                    th::bg_dark()
                } else {
                    th::text()
                })
                .into(),
            index == device,
            message(RackEdit::Select {
                chain,
                device: index,
            }),
            accent,
        ));
    }
    if held.len() < MAX_CHAIN_DEVICES {
        let choices: Vec<Insert> = EffectType::all()
            .iter()
            .copied()
            .filter(|effect_type| *effect_type != EffectType::Rack)
            .map(Insert)
            .collect();
        devices = devices.push(
            pick_list(choices, None::<Insert>, move |choice: Insert| {
                message(RackEdit::AddDevice {
                    chain,
                    effect_type: choice.0,
                })
            })
            .placeholder("Add effect")
            .text_size(9)
            .padding([2, 5])
            .style(control_style::list(accent)),
        );
    }
    devices = devices.push(Space::with_width(Length::Fill));
    if let Some(slot) = held.get(device) {
        devices = devices
            .push(small_button(
                if slot.bypass { "Bypassed" } else { "Active" }.into(),
                (!slot.bypass).then_some(accent),
                message(RackEdit::ToggleDevice { chain, device }),
            ))
            .push(hint::with_hint(
                button(icons::icon(icons::X).size(11).color(th::text_dim()))
                    .on_press(message(RackEdit::RemoveDevice { chain, device }))
                    .padding([2, 5])
                    .style(control_style::style(None)),
                "Remove this effect",
            ));
    }

    // The chosen device's parameters, each with its macro menu.
    let mut grid = column![].spacing(4);
    if let Some(slot) = held.get(device) {
        let described = descriptors(slot.effect_type);
        for (line, group) in described.chunks(PARAMS_PER_ROW).enumerate() {
            let mut cells = row![].spacing(2);
            for (offset, descriptor) in group.iter().enumerate() {
                let index = line * PARAMS_PER_ROW + offset;
                let value = device_value(rack, &effect.params, chain, device, index, descriptor);
                let mapped = rack
                    .mapping_of(chain, device, index)
                    .map(|mapping| usize::from(mapping.macro_index));
                let mut control = EffectKnobWidget::for_instrument(
                    track_id,
                    index,
                    value,
                    descriptor.min,
                    descriptor.max,
                    descriptor.default,
                    if mapped.is_some() {
                        th::text_muted()
                    } else {
                        accent
                    },
                );
                control.target = KnobTarget::Rack {
                    effect_id,
                    chain,
                    device,
                };
                let choices: Vec<MacroChoice> = std::iter::once(MacroChoice(None))
                    .chain((0..MACROS).map(|index| MacroChoice(Some(index))))
                    .collect();
                cells = cells.push(
                    column![
                        text(typography::fit(
                            descriptor.name,
                            PARAM_COL_W - 4.0,
                            9.0,
                            iced::Font::DEFAULT
                        ))
                        .size(9)
                        .color(th::text_dim())
                        .wrapping(text::Wrapping::None),
                        canvas(control)
                            .width(Length::Fixed(PARAM_KNOB))
                            .height(Length::Fixed(PARAM_KNOB)),
                        text(value_label(value, descriptor.unit))
                            .font(typography::PERFORM_TECH)
                            .size(9)
                            .color(th::text()),
                        pick_list(choices, Some(MacroChoice(mapped)), move |choice| {
                            message(RackEdit::Map {
                                chain,
                                device,
                                param: index,
                                macro_index: choice.0,
                            })
                        })
                        .text_size(8)
                        .padding([1, 3])
                        .style(control_style::list(accent)),
                    ]
                    .width(Length::Fixed(PARAM_COL_W))
                    .align_x(Alignment::Center),
                );
            }
            grid = grid.push(cells);
        }
    } else {
        grid = grid.push(
            text("An empty chain passes the signal. Add an effect.")
                .size(10)
                .color(th::text_muted()),
        );
    }

    column![
        container(macros).padding([4, 6]),
        rule(),
        container(chains).padding([3, 8]),
        container(devices).padding([3, 8]),
        quiet_scroll::vertical(container(grid).padding([2, 8])).height(Length::Fill),
    ]
    .into()
}
