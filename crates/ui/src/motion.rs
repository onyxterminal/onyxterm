#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::{ease_out_quint, Animation, AnimationExt, AnyElement, ElementId, IntoElement, Styled};
use std::time::Duration;

const MILLIS_PER_SECOND: f32 = 1000.0;

pub fn fade_in<E>(element: E, id: impl Into<ElementId>, animation_ms: f32) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if animation_ms <= 0.0 {
        return element.into_any_element();
    }
    let animation =
        Animation::new(Duration::from_secs_f32(animation_ms / MILLIS_PER_SECOND)).with_easing(ease_out_quint());
    element.with_animation(id, animation, |element, delta| element.opacity(delta)).into_any_element()
}
