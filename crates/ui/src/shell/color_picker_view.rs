use super::pane_drag::{bounds_probe, fraction, window_drag_listener};
use super::theme_editor::{ButtonTone, EditorButton};
use super::Shell;
use crate::color_picker::{ColorPicker, PickerDrag};
use crate::theme::custom::preset_colors;
use crate::theme::hsv::{Hsv, FULL_TURN_DEGREES};
use crate::theme::menu_chrome::{field_text, focused_field};
use gpui::{
    black, div, linear_color_stop, linear_gradient, point, prelude::*, px, white, BoxShadow, Context, CursorStyle, Div,
    Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, SharedString, Stateful,
};

const HEX_PLACEHOLDER: &str = "#RRGGBB";
const SPECTRUM_WIDTH_PX: f32 = 236.0;
const SPECTRUM_HEIGHT_PX: f32 = 156.0;
const HUE_TRACK_HEIGHT_PX: f32 = 12.0;
const SPECTRUM_THUMB_PX: f32 = 16.0;
const HUE_THUMB_PX: f32 = 18.0;
const HUE_SEGMENTS: usize = 6;
const COMPARISON_HEIGHT_PX: f32 = 40.0;
const PRESET_SWATCH_PX: f32 = 22.0;
const HEX_FIELD_HEIGHT_PX: f32 = 32.0;
const THUMB_SHADOW_ALPHA: f32 = 0.35;
const GRADIENT_RIGHT_DEGREES: f32 = 90.0;
const GRADIENT_DOWN_DEGREES: f32 = 180.0;

fn picker_thumb(center: Point<Pixels>, fill: Hsla, diameter: Pixels) -> Div {
    let shadow = BoxShadow {
        color: black().opacity(THUMB_SHADOW_ALPHA),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(3.0),
        spread_radius: px(0.0),
    };
    div().absolute().left(center.x - diameter / 2.0).top(center.y - diameter / 2.0)
        .size(diameter).rounded_full().bg(fill)
        .border_2().border_color(white()).shadow(vec![shadow])
}

fn hue_segment(index: usize) -> Div {
    let step = FULL_TURN_DEGREES / HUE_SEGMENTS as f32;
    let stop = |position: usize| Hsv::new(position as f32 * step, 1.0, 1.0).to_hsla();
    let gradient = linear_gradient(
        GRADIENT_RIGHT_DEGREES,
        linear_color_stop(stop(index), 0.0),
        linear_color_stop(stop(index + 1), 1.0),
    );
    div().flex_1().h_full().bg(gradient)
}

impl Shell {
    fn begin_picker_drag(&mut self, drag: PickerDrag, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(picker) = self.state.color_picker.as_mut() else { return };
        picker.begin_drag(drag);
        self.drag_picker_to(position, cx);
    }

    fn drag_picker_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(picker) = self.state.color_picker.as_mut() else { return };
        let Some(bounds) = picker.drag().and_then(|drag| self.picker_bounds.borrow().get(&drag).copied()) else {
            return;
        };
        let local = position - bounds.origin;
        picker.drag_to(fraction(local.x, bounds.size.width), fraction(local.y, bounds.size.height));
        self.preview_picker_color(cx);
        self.sync_hex_field();
    }

    fn move_picker_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.release_picker_drag(cx);
            return;
        }
        self.drag_picker_to(event.position, cx);
    }

    fn release_picker_drag(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.state.color_picker.as_mut() else { return };
        picker.end_drag();
        self.persist_picker_color();
        cx.notify();
    }

    pub(super) fn picker_drag_listener(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        self.state.color_picker.as_ref().and_then(ColorPicker::drag)?;
        Some(window_drag_listener(cx, Shell::move_picker_drag, Shell::release_picker_drag))
    }

    pub(super) fn render_color_picker(&self, picker: &ColorPicker, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let cancel = EditorButton { id: "picker-cancel", label: "Cancel".into(), tone: ButtonTone::Quiet };
        let keep = EditorButton { id: "picker-keep", label: "Keep color".into(), tone: ButtonTone::Primary };
        div().mt(px(4.0 * scale)).mb(px(8.0 * scale)).p(px(14.0 * scale))
            .rounded(px(10.0 * scale)).bg(theme.surface.subtle)
            .border_1().border_color(theme.border.muted)
            .flex().flex_col().gap(px(14.0 * scale))
            .child(div().flex().gap(px(18.0 * scale))
                .child(div().flex_none().flex().flex_col().gap(px(14.0 * scale))
                    .child(self.color_spectrum(picker, cx))
                    .child(self.hue_slider(picker, cx)))
                .child(div().flex_1().min_w(px(0.0)).flex().flex_col().gap(px(12.0 * scale))
                    .child(self.color_comparison(picker, cx))
                    .child(self.hex_input())
                    .child(self.preset_swatches(picker, cx))))
            .child(div().flex().justify_end().gap(px(8.0 * scale))
                .child(self.editor_button(cancel, |shell, cx| shell.cancel_color_picker(cx), cx))
                .child(self.editor_button(keep, |shell, cx| shell.finish_color_picker(cx), cx)))
    }

    fn color_spectrum(&self, picker: &ColorPicker, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let (width, height) = (px(SPECTRUM_WIDTH_PX * scale), px(SPECTRUM_HEIGHT_PX * scale));
        let radius = px(8.0 * scale);
        let color = picker.color();
        let layer = || div().absolute().inset_0().rounded(radius);
        let whiten = linear_gradient(
            GRADIENT_RIGHT_DEGREES,
            linear_color_stop(white(), 0.0),
            linear_color_stop(white().opacity(0.0), 1.0),
        );
        let darken = linear_gradient(
            GRADIENT_DOWN_DEGREES,
            linear_color_stop(black().opacity(0.0), 0.0),
            linear_color_stop(black(), 1.0),
        );
        let thumb_center = point(width * color.saturation, height * (1.0 - color.value));
        div().id("color-spectrum").relative().flex_none().w(width).h(height)
            .rounded(radius).bg(color.pure_hue()).cursor(CursorStyle::Crosshair)
            .child(layer().bg(whiten))
            .child(layer().bg(darken))
            .child(layer().border_1().border_color(theme.border.muted))
            .child(bounds_probe(&self.picker_bounds, PickerDrag::Spectrum))
            .child(picker_thumb(thumb_center, color.to_hsla(), px(SPECTRUM_THUMB_PX * scale)))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                shell.begin_picker_drag(PickerDrag::Spectrum, event.position, cx);
            }))
    }

    fn hue_slider(&self, picker: &ColorPicker, cx: &mut Context<Self>) -> Stateful<Div> {
        let scale = self.state.settings.ui_scale;
        let (width, height) = (px(SPECTRUM_WIDTH_PX * scale), px(HUE_TRACK_HEIGHT_PX * scale));
        let radius = height / 2.0;
        let color = picker.color();
        let last = HUE_SEGMENTS - 1;
        let segments = (0..HUE_SEGMENTS).map(|index| hue_segment(index)
            .when(index == 0, |segment| segment.rounded_l(radius))
            .when(index == last, |segment| segment.rounded_r(radius)));
        let thumb_center = point(width * (color.hue / FULL_TURN_DEGREES), height / 2.0);
        div().id("hue-slider").relative().flex_none().w(width).h(height)
            .flex().flex_row().cursor_pointer()
            .children(segments)
            .child(bounds_probe(&self.picker_bounds, PickerDrag::Hue))
            .child(picker_thumb(thumb_center, color.pure_hue(), px(HUE_THUMB_PX * scale)))
            .on_mouse_down(MouseButton::Left, cx.listener(|shell, event: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                shell.begin_picker_drag(PickerDrag::Hue, event.position, cx);
            }))
    }

    fn color_comparison(&self, picker: &ColorPicker, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let radius = px(8.0 * scale);
        let original = picker.original();
        div().flex().flex_col().gap(px(5.0 * scale))
            .child(div().flex().justify_between()
                .text_size(px(theme.ui_font.hint_px)).text_color(theme.text.muted)
                .child("Original, click to restore").child("New"))
            .child(div().flex().h(px(COMPARISON_HEIGHT_PX * scale)).rounded(radius)
                .border_1().border_color(theme.border.default)
                .child(div().id("original-color").flex_1().h_full().rounded_l(radius)
                    .bg(original.to_hsla()).cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                        cx.stop_propagation();
                        shell.choose_picker_color(original, cx);
                    })))
                .child(div().flex_1().h_full().rounded_r(radius).bg(picker.color().to_hsla())))
    }

    fn hex_input(&self) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        focused_field(theme).h(px(HEX_FIELD_HEIGHT_PX * scale)).px(px(10.0 * scale))
            .flex().items_center().rounded(px(6.0 * scale))
            .font_family(SharedString::from(theme.font.family.clone()))
            .child(field_text(&self.state.theme_color_input, HEX_PLACEHOLDER, theme))
    }

    fn preset_swatches(&self, picker: &ColorPicker, cx: &mut Context<Self>) -> Div {
        let theme = &self.state.theme;
        let scale = self.state.settings.ui_scale;
        let current = picker.color().to_rgb();
        let swatches = preset_colors(picker.field()).into_iter().enumerate().map(|(index, preset)| {
            let ring = if preset == current { theme.accent.primary } else { theme.border.default };
            div().id(("preset-color", index)).size(px(PRESET_SWATCH_PX * scale)).rounded_full()
                .bg(preset.to_hsla()).border_2().border_color(ring).cursor_pointer()
                .on_mouse_down(MouseButton::Left, cx.listener(move |shell, _event, _window, cx| {
                    cx.stop_propagation();
                    shell.choose_picker_color(preset, cx);
                }))
        }).collect::<Vec<_>>();
        div().flex().flex_col().gap(px(6.0 * scale))
            .child(div().text_size(px(theme.ui_font.hint_px)).text_color(theme.text.muted).child("From presets"))
            .child(div().flex().flex_wrap().gap(px(6.0 * scale)).children(swatches))
    }
}
