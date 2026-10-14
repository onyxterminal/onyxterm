use super::color_picker_view::picker_thumb;
use super::pane_drag::{bounds_probe, fraction, window_drag_listener};
use super::settings_view::CAPTION_PX;
use super::Shell;
use crate::settings::{BUNDLE_SIZE_PERCENT, EDITOR_BUNDLE_SIZE_KEY, UI_SCALE_KEY, UI_SCALE_PERCENT};
use crate::slider::SliderRange;
use gpui::{div, point, prelude::*, px, white, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
    Stateful};

const PERCENT: f32 = 100.0;
const TRACK_WIDTH_PX: f32 = 170.0;
const TRACK_HEIGHT_PX: f32 = 4.0;
const THUMB_PX: f32 = 16.0;
const VALUE_WIDTH_PX: f32 = 40.0;
const SMALL_GLYPH_PX: f32 = 10.0;
const LARGE_GLYPH_PX: f32 = 15.0;
const SIZE_GLYPH: &str = "A";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SettingSlider {
    InterfaceSize,
    EditorSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SliderDrag {
    slider: SettingSlider,
    pending_percent: Option<f32>,
}

impl SettingSlider {
    fn applies_while_dragging(self) -> bool {
        self == Self::EditorSize
    }

    fn range(self) -> SliderRange {
        match self {
            Self::InterfaceSize => UI_SCALE_PERCENT,
            Self::EditorSize => BUNDLE_SIZE_PERCENT,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::InterfaceSize => UI_SCALE_KEY,
            Self::EditorSize => EDITOR_BUNDLE_SIZE_KEY,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::InterfaceSize => "interface-size-slider",
            Self::EditorSize => "editor-size-slider",
        }
    }
}

impl Shell {
    fn slider_percent(&self, slider: SettingSlider) -> f32 {
        let pending = self.slider_drag.filter(|drag| drag.slider == slider).and_then(|drag| drag.pending_percent);
        if let Some(percent) = pending {
            return percent;
        }
        let settings = &self.state.settings;
        let share = match slider {
            SettingSlider::InterfaceSize => settings.ui_scale,
            SettingSlider::EditorSize => settings.editor_bundle_size,
        };
        share * PERCENT
    }

    fn set_slider_percent(&mut self, slider: SettingSlider, percent: f32) {
        let settings = &mut self.state.settings;
        match slider {
            SettingSlider::InterfaceSize => settings.ui_scale = percent / PERCENT,
            SettingSlider::EditorSize => settings.editor_bundle_size = percent / PERCENT,
        }
    }

    fn begin_slider_drag(&mut self, slider: SettingSlider, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.slider_drag = Some(SliderDrag { slider, pending_percent: None });
        self.state.settings_error = None;
        self.drag_slider_to(position, cx);
    }

    fn drag_slider_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.slider_drag else { return };
        let slider = drag.slider;
        let Some(bounds) = self.slider_bounds.borrow().get(&slider).copied() else { return };
        let percent = slider.range().value_at(fraction(position.x - bounds.origin.x, bounds.size.width));
        if percent == self.slider_percent(slider) {
            return;
        }
        if slider.applies_while_dragging() {
            self.set_slider_percent(slider, percent);
            self.apply_settings(cx);
        } else {
            self.slider_drag = Some(SliderDrag { pending_percent: Some(percent), ..drag });
        }
        cx.notify();
    }

    fn move_slider_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.release_slider_drag(cx);
            return;
        }
        self.drag_slider_to(event.position, cx);
    }

    fn release_slider_drag(&mut self, cx: &mut Context<Self>) {
        let Some(SliderDrag { slider, pending_percent }) = self.slider_drag.take() else { return };
        if let Some(percent) = pending_percent {
            self.set_slider_percent(slider, percent);
            self.apply_settings(cx);
        }
        if let Some(value) = self.state.settings.persisted_value(slider.key()) {
            self.save_setting(slider.key(), &value);
        }
        cx.notify();
    }

    pub(super) fn slider_drag_listener(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        self.slider_drag?;
        Some(window_drag_listener(cx, Shell::move_slider_drag, Shell::release_slider_drag))
    }

    pub(super) fn interface_size_slider(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let glyph = |size_px: f32| div().text_size(px(size_px * scale)).text_color(theme.text.muted).child(SIZE_GLYPH);
        div().flex_none().flex().items_center().gap(px(12.0 * scale))
            .child(glyph(SMALL_GLYPH_PX))
            .child(self.slider_track(SettingSlider::InterfaceSize, cx))
            .child(glyph(LARGE_GLYPH_PX))
            .child(self.slider_value(SettingSlider::InterfaceSize))
    }

    pub(super) fn setting_slider(&self, slider: SettingSlider, cx: &mut Context<Self>) -> Div {
        let scale = self.state.settings.ui_scale;
        div().flex_none().flex().items_center().gap(px(12.0 * scale))
            .child(self.slider_track(slider, cx))
            .child(self.slider_value(slider))
    }

    fn slider_value(&self, slider: SettingSlider) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        div().w(px(VALUE_WIDTH_PX * scale)).flex().justify_end()
            .text_size(px(CAPTION_PX * scale)).text_color(theme.text.default)
            .child(format!("{}%", self.slider_percent(slider).round() as u32))
    }

    fn slider_track(&self, slider: SettingSlider, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (width, thumb, rail) = (px(TRACK_WIDTH_PX * scale), px(THUMB_PX * scale), px(TRACK_HEIGHT_PX * scale));
        let filled = width * slider.range().fraction_of(self.slider_percent(slider));
        let rail_top = (thumb - rail) / 2.0;
        div().id(slider.id()).relative().flex_none().w(width).h(thumb).cursor_pointer()
            .child(div().absolute().left_0().top(rail_top).w(width).h(rail).rounded_full()
                .bg(theme.chrome.icon_hover_fill))
            .child(div().absolute().left_0().top(rail_top).w(filled).h(rail).rounded_full().bg(theme.accent.primary))
            .child(bounds_probe(&self.slider_bounds, slider))
            .child(picker_thumb(point(filled, thumb / 2.0), white(), thumb))
            .on_mouse_down(MouseButton::Left, cx.listener(move |shell, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                shell.begin_slider_drag(slider, event.position, cx);
            }))
    }
}
