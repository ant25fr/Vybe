//! The MIDI effects card, at the head of an instrument track's devices.

use iced::widget::{button, column, container, pick_list, row, text};
use iced::{Element, Length};
use vibez_core::midi_effects::{ArpPattern, ArpRate, Arpeggiator, MidiEffects};
use vibez_core::scale::{ChordKind, Scale, ScaleKind, ROOT_NAMES};

use crate::domains::arrangement::ArrangementMsg;
use crate::message::Message;
use crate::state::UiTrack;
use crate::theme as th;

use super::*;

pub(super) const MIDI_CARD_W: f32 = 200.0;

/// A menu entry that may be "Off".
#[derive(Debug, Clone, Copy, PartialEq)]
struct Optional<T>(Option<T>);

impl std::fmt::Display for Optional<ArpRate> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(rate) => rate.fmt(f),
            None => f.write_str("Off"),
        }
    }
}

impl std::fmt::Display for Optional<u8> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            self.0
                .map_or("Off", |root| ROOT_NAMES[usize::from(root % 12)]),
        )
    }
}

fn heading(label: &str) -> Element<'_, Message> {
    text(label).size(10).color(th::text_dim()).into()
}

impl App {
    pub(super) fn view_midi_effects_card(&self, track: &UiTrack) -> Element<'_, Message> {
        let effects = track.midi_effects;
        let id = track.id;
        let set = move |effects: MidiEffects| {
            Message::Arrangement(ArrangementMsg::SetMidiEffects(id, effects))
        };
        let chord = pick_list(ChordKind::ALL, Some(effects.chord), move |chord| {
            set(MidiEffects { chord, ..effects })
        })
        .text_size(10)
        .padding([2, 6]);

        let kind = effects.scale.map(|scale| scale.kind).unwrap_or_default();
        let roots: Vec<Optional<u8>> = std::iter::once(Optional(None))
            .chain((0..12).map(|root| Optional(Some(root))))
            .collect();
        let mut scale_row = row![pick_list(
            roots,
            Some(Optional(effects.scale.map(|scale| scale.root))),
            move |choice: Optional<u8>| {
                set(MidiEffects {
                    scale: choice.0.map(|root| Scale { root, kind }),
                    ..effects
                })
            },
        )
        .text_size(10)
        .padding([2, 6])]
        .spacing(4);
        if let Some(scale) = effects.scale {
            scale_row = scale_row.push(
                pick_list(ScaleKind::ALL, Some(scale.kind), move |kind| {
                    set(MidiEffects {
                        scale: Some(Scale { kind, ..scale }),
                        ..effects
                    })
                })
                .text_size(10)
                .padding([2, 6]),
            );
        }

        let arp = effects.arpeggiator;
        let rates: Vec<Optional<ArpRate>> = std::iter::once(Optional(None))
            .chain(ArpRate::ALL.into_iter().map(|rate| Optional(Some(rate))))
            .collect();
        let mut arp_column = column![pick_list(
            rates,
            Some(Optional(arp.map(|arp| arp.rate))),
            move |choice: Optional<ArpRate>| {
                set(MidiEffects {
                    arpeggiator: choice.0.map(|rate| Arpeggiator {
                        rate,
                        ..arp.unwrap_or_default()
                    }),
                    ..effects
                })
            },
        )
        .text_size(10)
        .padding([2, 6])]
        .spacing(4);
        if let Some(arp) = arp {
            let with = move |arp: Arpeggiator| {
                set(MidiEffects {
                    arpeggiator: Some(arp),
                    ..effects
                })
            };
            let pattern = pick_list(ArpPattern::ALL, Some(arp.pattern), move |pattern| {
                with(Arpeggiator { pattern, ..arp })
            })
            .text_size(10)
            .padding([2, 6]);
            let octaves = pick_list([1u8, 2, 3, 4], Some(arp.octaves), move |octaves| {
                with(Arpeggiator { octaves, ..arp })
            })
            .text_size(10)
            .padding([2, 6]);
            let mut gates = row![].spacing(2);
            for gate in [25u8, 50, 75, 100] {
                gates = gates.push(
                    button(text(format!("{gate}%")).size(9))
                        .on_press(with(Arpeggiator { gate, ..arp }))
                        .padding([2, 4])
                        .style(crate::widgets::control_style::style(
                            (arp.gate == gate).then(th::accent),
                        )),
                );
            }
            arp_column = arp_column
                .push(row![pattern, octaves].spacing(4))
                .push(gates);
        }

        container(
            column![
                text("MIDI Effects").size(11).color(th::text()),
                heading("Chord"),
                chord,
                heading("Scale"),
                scale_row,
                heading("Arpeggiator"),
                arp_column,
            ]
            .spacing(4),
        )
        .padding(8)
        .width(MIDI_CARD_W)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(th::bg_elevated().into()),
            border: iced::Border {
                color: th::border(),
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
    }
}
