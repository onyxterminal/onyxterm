use super::{box_shadows, Theme};
use gpui::{div, prelude::*, px, Div, ElementId, FontWeight, Hsla, SharedString, Stateful};

pub const CARET: &str = "\u{258f}";
const SEPARATOR_MARGIN_Y_PX: f32 = 5.0;
const SEPARATOR_THICKNESS_PX: f32 = 1.0;
const FOCUS_RING_ALPHA: f32 = 0.25;
const FOCUS_RING_SPREAD_PX: f32 = 3.0;

pub struct MenuRowStyle {
    pub group: SharedString,
    pub is_highlighted: bool,
    pub is_destructive: bool,
}

pub fn menu_panel(theme: &Theme) -> Div {
    let menu = theme.menu;
    div()
        .w(px(menu.width_px))
        .p(px(menu.padding_px))
        .font_family(SharedString::from(theme.ui_font.family.clone()))
        .text_size(px(theme.ui_font.menu_px))
        .text_color(theme.text.default)
        .bg(theme.chrome.menu_fill)
        .border(px(menu.border_px))
        .border_color(theme.chrome.menu_border)
        .rounded(px(menu.radius_px))
        .shadow(box_shadows(&theme.shadow.menu))
        .flex()
        .flex_col()
}

pub fn menu_row(id: impl Into<ElementId>, style: MenuRowStyle, theme: &Theme) -> Stateful<Div> {
    let menu = theme.menu;
    let (highlight, inverse) = (theme.accent.primary, theme.text.inverse);
    let text_color = match style.is_destructive {
        true => theme.chrome.destructive,
        false => theme.text.default,
    };
    div()
        .id(id)
        .group(style.group)
        .h(px(menu.row_height_px))
        .flex_none()
        .px(px(menu.row_padding_x_px))
        .flex()
        .flex_row()
        .items_center()
        .rounded(px(menu.row_radius_px))
        .cursor_pointer()
        .text_color(text_color)
        .when(style.is_highlighted, |row| row.bg(highlight).text_color(inverse))
        .hover(move |style| style.bg(highlight).text_color(inverse))
}

pub fn menu_shortcut(text: impl Into<SharedString>, style: &MenuRowStyle, theme: &Theme) -> Div {
    let selected = theme.chrome.selected_shortcut;
    let color = match style.is_highlighted {
        true => selected,
        false => theme.text.subtle,
    };
    div()
        .flex_none()
        .text_size(px(theme.ui_font.shortcut_px))
        .text_color(color)
        .group_hover(style.group.clone(), move |style| style.text_color(selected))
        .child(text.into())
}

pub fn menu_separator(theme: &Theme) -> Div {
    let inset = px(theme.menu.row_padding_x_px);
    div().flex_none().h(px(SEPARATOR_THICKNESS_PX)).mx(inset).my(px(SEPARATOR_MARGIN_Y_PX)).bg(theme.border.muted)
}

pub fn menu_section_label(text: &'static str, theme: &Theme) -> Div {
    div()
        .h(px(theme.menu.section_label_height_px))
        .px(px(theme.menu.row_padding_x_px))
        .flex()
        .items_center()
        .text_size(px(theme.ui_font.section_label_px))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.subtle)
        .child(text)
}

pub fn focused_field(theme: &Theme) -> Div {
    div()
        .bg(theme.surface.canvas)
        .border_1()
        .border_color(theme.accent.primary)
        .shadow(vec![focus_ring(theme.accent.primary)])
}

fn focus_ring(accent: Hsla) -> gpui::BoxShadow {
    gpui::BoxShadow {
        color: accent.opacity(FOCUS_RING_ALPHA),
        offset: gpui::point(px(0.0), px(0.0)),
        blur_radius: px(0.0),
        spread_radius: px(FOCUS_RING_SPREAD_PX),
    }
}

pub fn caret_text(text: &str, placeholder: &str, theme: &Theme) -> Div {
    let caret = div().text_color(theme.accent.primary).child(CARET);
    let field = div().flex().flex_row().items_center();
    if text.is_empty() {
        return field.child(caret).child(div().text_color(theme.text.subtle).child(placeholder.to_string()));
    }
    field.child(div().text_color(theme.text.default).child(text.to_string())).child(caret)
}
