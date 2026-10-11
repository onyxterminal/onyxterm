#![deny(warnings)]
#![forbid(unsafe_code)]

use gpui::{AnyElement, IntoElement};
use std::time::Duration;

const MILLIS_PER_SECOND: f32 = 1000.0;

pub fn fade_in<E>(element: E, _id: impl Into<gpui::ElementId>, animation_ms: f32) -> AnyElement
where
    E: IntoElement + 'static,
{
    let _ = Duration::from_secs_f32(animation_ms / MILLIS_PER_SECOND);
    element.into_any_element()
}
