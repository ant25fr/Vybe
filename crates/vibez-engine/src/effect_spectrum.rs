//! Bounded selected-insert sample delivery, independent of whole-channel spectra.

use rtrb::{Consumer, Producer, RingBuffer};
use vibez_core::id::{EffectId, TrackId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectSpectrumPosition {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSpectrumTarget {
    pub track_id: TrackId,
    pub effect_id: EffectId,
    pub position: EffectSpectrumPosition,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectSpectrumSample {
    pub generation: u64,
    pub value: f32,
}

pub(crate) struct EffectSpectrumTap {
    target: Option<EffectSpectrumTarget>,
    producer: Producer<EffectSpectrumSample>,
    /// Deliver (L - R) / 2 instead of the mono sum: what a stereo widener
    /// changes, and what the sum cancels.
    side: bool,
}

impl EffectSpectrumTap {
    pub(crate) fn new(capacity: usize) -> (Self, Consumer<EffectSpectrumSample>) {
        Self::with_side(capacity, false)
    }

    /// A tap that delivers the side signal, (L - R) / 2.
    pub(crate) fn new_side(capacity: usize) -> (Self, Consumer<EffectSpectrumSample>) {
        Self::with_side(capacity, true)
    }

    fn with_side(capacity: usize, side: bool) -> (Self, Consumer<EffectSpectrumSample>) {
        let (producer, consumer) = RingBuffer::new(capacity);
        (
            Self {
                target: None,
                producer,
                side,
            },
            consumer,
        )
    }

    pub(crate) fn select(&mut self, target: Option<EffectSpectrumTarget>) {
        self.target = target;
    }

    pub(crate) fn observe(
        &mut self,
        track_id: TrackId,
        effect_id: EffectId,
        position: EffectSpectrumPosition,
        samples: &[f32],
        channels: usize,
    ) {
        let Some(target) = self.target.filter(|target| {
            target.track_id == track_id
                && target.effect_id == effect_id
                && target.position == position
        }) else {
            return;
        };
        if channels == 0 {
            return;
        }
        for frame in samples.chunks_exact(channels) {
            let value = if channels > 1 {
                if self.side {
                    (frame[0] - frame[1]) * 0.5
                } else {
                    (frame[0] + frame[1]) * 0.5
                }
            } else if self.side {
                0.0
            } else {
                frame[0]
            };
            if self
                .producer
                .push(EffectSpectrumSample {
                    generation: target.generation,
                    value,
                })
                .is_err()
            {
                // A blocked analyser must never back-pressure the audible path.
                break;
            }
        }
    }
}
