//! Moving a shape's vertices by its sliders.

use crate::osd::Diff;
use crate::preset::Preset;
use crate::set::{Slider, SliderSet};

/// How far a slider is applied, as a fraction, for a value in percent. An
/// inverted slider's list is applied as the slider goes down: full at 0,
/// nothing at 100.
pub fn applied(slider: &Slider, percent: f32) -> f32 {
    let mut fraction = percent / 100.0;
    if slider.clamp {
        fraction = fraction.clamp(0.0, 1.0);
    }
    if slider.invert {
        1.0 - fraction
    } else {
        fraction
    }
}

/// Where each slider of a set stands, in percent, for the lightest (`big`
/// false) or the heaviest body: the preset's value where it gives one, the
/// set's own default where it does not.
pub fn values(set: &SliderSet, preset: Option<&Preset>, big: bool) -> Vec<f32> {
    set.sliders
        .iter()
        .map(|s| {
            preset
                .and_then(|p| p.value(&s.name, big))
                .unwrap_or(if big { s.big } else { s.small })
        })
        .collect()
}

/// Add one list to a shape's positions, `amount` of the way.
pub fn add(positions: &mut [[f32; 3]], diffs: &[Diff], amount: f32) {
    if amount == 0.0 {
        return;
    }
    for d in diffs {
        if let Some(p) = positions.get_mut(d.vertex as usize) {
            for (c, o) in p.iter_mut().zip(d.offset) {
                *c += o * amount;
            }
        }
    }
}

/// Add one list to a shape's texture coordinates, `amount` of the way. A
/// list for coordinates uses the first two of each offset's three numbers.
pub fn add_uv(uvs: &mut [[f32; 2]], diffs: &[Diff], amount: f32) {
    if amount == 0.0 {
        return;
    }
    for d in diffs {
        if let Some(uv) = uvs.get_mut(d.vertex as usize) {
            uv[0] += d.offset[0] * amount;
            uv[1] += d.offset[1] * amount;
        }
    }
}

/// Whether a list used to take vertices out marks this one: any offset at
/// all does. Such a list can name every vertex and mark only some.
pub fn marks(diff: &Diff) -> bool {
    diff.offset != [0.0; 3]
}
