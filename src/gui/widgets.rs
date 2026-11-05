//! The website's `components/kit`, painted by hand: Material 3 pill buttons with
//! state layers, chips, cards, the navigation rail items, switches and tabs.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Frame, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind,
    Ui, Vec2, WidgetText,
};

use super::icons::Icon;
use super::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Filled,
    Tonal,
    Outlined,
    Text,
    Danger,
}

/// `kit/button.tsx`
#[must_use]
pub struct Button {
    text: String,
    icon: Option<Icon>,
    kind: Kind,
    enabled: bool,
    small: bool,
    corners: CornerRadius,
    progress: Option<f32>,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon: None,
            kind: Kind::Filled,
            enabled: true,
            small: false,
            corners: CornerRadius::same(SHAPE_FULL),
            progress: None,
        }
    }

    pub fn filled(text: impl Into<String>) -> Self {
        Self::new(text)
    }
    pub fn tonal(text: impl Into<String>) -> Self {
        Self::new(text).kind(Kind::Tonal)
    }
    pub fn outlined(text: impl Into<String>) -> Self {
        Self::new(text).kind(Kind::Outlined)
    }
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(text).kind(Kind::Text)
    }
    pub fn danger(text: impl Into<String>) -> Self {
        Self::new(text).kind(Kind::Danger)
    }

    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn corners(mut self, corners: CornerRadius) -> Self {
        self.corners = corners;
        self
    }
    /// Fills the button from the left like a progress bar (0..=1).
    pub fn progress(mut self, progress: Option<f32>) -> Self {
        self.progress = progress;
        self
    }

    fn colors(&self) -> (Color32, Color32, Option<Stroke>) {
        if !self.enabled {
            let bg = match self.kind {
                Kind::Filled | Kind::Tonal => alpha(ON_SURFACE, 0.12),
                _ => Color32::TRANSPARENT,
            };
            let stroke = matches!(self.kind, Kind::Outlined | Kind::Danger).then(|| Stroke::new(1.0, alpha(ON_SURFACE, 0.12)));
            return (bg, alpha(ON_SURFACE, 0.38), stroke);
        }
        match self.kind {
            Kind::Filled => (PRIMARY, ON_PRIMARY, None),
            Kind::Tonal => (SECONDARY_CONTAINER, ON_SECONDARY_CONTAINER, None),
            Kind::Outlined => (Color32::TRANSPARENT, PRIMARY, Some(Stroke::new(1.0, OUTLINE))),
            Kind::Text => (Color32::TRANSPARENT, PRIMARY, None),
            Kind::Danger => (Color32::TRANSPARENT, ERROR, Some(Stroke::new(1.0, alpha(ERROR, 0.6)))),
        }
    }

    pub fn ui(self, ui: &mut Ui) -> Response {
        let (container, content, stroke) = self.colors();
        let height = if self.small { 32.0 } else { 40.0 };
        let icon_size = if self.small { 16.0 } else { 18.0 };
        let galley = ui.painter().layout_no_wrap(self.text.clone(), font_bold(14.0), content);
        let (pad_l, pad_r) = match (self.kind, self.icon.is_some(), self.text.is_empty()) {
            (_, true, true) => (10.0, 10.0),
            (Kind::Text, true, _) => (12.0, 16.0),
            (Kind::Text, false, _) => (12.0, 12.0),
            (_, true, _) => (16.0, 24.0),
            _ => (24.0, 24.0),
        };
        let (pad_l, pad_r) = if self.small { (pad_l * 0.6, pad_r * 0.6) } else { (pad_l, pad_r) };
        let icon_w = self.icon.map_or(0.0, |_| icon_size + if self.text.is_empty() { 0.0 } else { 8.0 });
        let width = pad_l + icon_w + galley.size().x + pad_r;
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);

        if ui.is_rect_visible(rect) {
            let pressed = response.is_pointer_button_down_on();
            let rect = if pressed { rect.shrink(rect.height() * 0.015) } else { rect };
            let painter = ui.painter();
            painter.rect_filled(rect, self.corners, container);
            if let Some(p) = self.progress {
                let mut filled = rect;
                filled.set_width(rect.width() * p.clamp(0.0, 1.0));
                painter.with_clip_rect(filled).rect_filled(rect, self.corners, alpha(content, 0.25));
            }
            if self.enabled && (response.hovered() || pressed) {
                let amount = if pressed { STATE_PRESSED } else { STATE_HOVER };
                painter.rect_filled(rect, self.corners, alpha(content, amount));
            }
            if let Some(stroke) = stroke {
                painter.rect_stroke(rect, self.corners, stroke, StrokeKind::Inside);
            }
            let mut x = rect.left() + pad_l;
            if let Some(icon) = self.icon {
                let r = Rect::from_center_size(Pos2::new(x + icon_size / 2.0, rect.center().y), Vec2::splat(icon_size));
                icon.paint(ui, r, content);
                x += icon_w;
            }
            let pos = Pos2::new(x, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(pos, galley, content);
        }
        if self.enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

/// Round icon-only button (`kit/iconButton.tsx`).
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button_colored(ui, icon, tooltip, ON_SURFACE_VARIANT)
}

pub fn icon_button_colored(ui: &mut Ui, icon: Icon, tooltip: &str, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if response.hovered() || response.is_pointer_button_down_on() {
            let amount = if response.is_pointer_button_down_on() { STATE_PRESSED } else { STATE_HOVER };
            ui.painter().circle_filled(rect.center(), 20.0, alpha(ON_SURFACE, amount));
        }
        icon.paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(22.0)), color);
    }
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tooltip)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipStyle {
    Plain,
    Highlight,
    Success,
}

/// Small information chip (`kit/chip.tsx`, `small`).
pub fn chip(ui: &mut Ui, icon: Option<Icon>, text: &str, style: ChipStyle) -> Response {
    let (bg, fg, icon_color) = match style {
        ChipStyle::Plain => (Color32::TRANSPARENT, ON_SURFACE_VARIANT, PRIMARY),
        ChipStyle::Highlight => (SECONDARY_CONTAINER, ON_SECONDARY_CONTAINER, ON_SECONDARY_CONTAINER),
        ChipStyle::Success => (PRIMARY_CONTAINER, ON_PRIMARY_CONTAINER, ON_PRIMARY_CONTAINER),
    };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_bold(12.0), fg);
    let icon_w = if icon.is_some() { 14.0 + 4.0 } else { 0.0 };
    let pad_l = if icon.is_some() { 6.0 } else { 8.0 };
    let size = Vec2::new(pad_l + icon_w + galley.size().x + 8.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let r = CornerRadius::same(SHAPE_SM);
        ui.painter().rect_filled(rect, r, bg);
        if style == ChipStyle::Plain {
            ui.painter().rect_stroke(rect, r, Stroke::new(1.0, OUTLINE_VARIANT), StrokeKind::Inside);
        }
        let mut x = rect.left() + pad_l;
        if let Some(icon) = icon {
            icon.paint(ui, Rect::from_min_size(Pos2::new(x, rect.center().y - 7.0), Vec2::splat(14.0)), icon_color);
            x += icon_w;
        }
        ui.painter().galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    }
    response
}

/// Small filter chip with an icon and an optional count, as the website's
/// filter sidebar (`FilterChip small`): the icon becomes a check mark when selected.
pub fn filter_chip_small(ui: &mut Ui, selected: bool, icon: Icon, text: &str, count: Option<usize>) -> Response {
    let fg = if selected { ON_SECONDARY_CONTAINER } else { ON_SURFACE_VARIANT };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_bold(12.0), fg);
    let count_galley = count.map(|c| ui.painter().layout_no_wrap(c.to_string(), font(12.0), alpha(fg, 0.7)));
    let count_w = count_galley.as_ref().map_or(0.0, |g| g.size().x + 6.0);
    let size = Vec2::new(6.0 + 14.0 + 4.0 + galley.size().x + count_w + 8.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let r = CornerRadius::same(SHAPE_SM);
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, r, SECONDARY_CONTAINER);
        } else {
            p.rect_stroke(rect, r, Stroke::new(1.0, OUTLINE_VARIANT), StrokeKind::Inside);
        }
        if response.hovered() {
            p.rect_filled(rect, r, alpha(fg, STATE_HOVER));
        }
        let icon_rect = Rect::from_min_size(Pos2::new(rect.left() + 6.0, rect.center().y - 7.0), Vec2::splat(14.0));
        if selected {
            Icon::Check.paint(ui, icon_rect, fg);
        } else {
            icon.paint(ui, icon_rect, PRIMARY);
        }
        let x = icon_rect.right() + 4.0;
        let text_w = galley.size().x;
        ui.painter().galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, fg);
        if let Some(g) = count_galley {
            let pos = Pos2::new(x + text_w + 6.0, rect.center().y - g.size().y / 2.0);
            ui.painter().galley(pos, g, fg);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A row of a drop-down menu with an icon.
pub fn menu_item(ui: &mut Ui, icon: Icon, text: &str, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 36.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(4), alpha(ON_SURFACE, STATE_HOVER));
        }
        icon.paint(ui, Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.center().y - 10.0), Vec2::splat(20.0)), color);
        ui.painter().text(Pos2::new(rect.left() + 40.0, rect.center().y), Align2::LEFT_CENTER, text, font(14.0), color);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// `kit/card.tsx`: translucent surface with a hairline outline.
pub fn card_frame() -> Frame {
    Frame::new()
        .fill(alpha(SURFACE_CONTAINER, 0.85))
        .corner_radius(CornerRadius::same(SHAPE_LG))
        .stroke(Stroke::new(1.0, alpha(OUTLINE_VARIANT, 0.5)))
        .inner_margin(Margin::same(24))
}

pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame()
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Navigation drawer item.
pub fn nav_item(ui: &mut Ui, icon: Icon, label: &str, selected: bool, badge: Option<&str>) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let r = CornerRadius::same(SHAPE_FULL);
        let fg = if selected { ON_SECONDARY_CONTAINER } else { ON_SURFACE_VARIANT };
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, r, SECONDARY_CONTAINER);
        }
        if response.hovered() || response.is_pointer_button_down_on() {
            let amount = if response.is_pointer_button_down_on() { STATE_PRESSED } else { STATE_HOVER };
            p.rect_filled(rect, r, alpha(ON_SURFACE, amount));
        }
        let icon_rect = Rect::from_min_size(Pos2::new(rect.left() + 16.0, rect.center().y - 12.0), Vec2::splat(24.0));
        icon.paint(ui, icon_rect, if selected { ON_SECONDARY_CONTAINER } else { ON_SURFACE_VARIANT });
        let font = if selected { font_bold(14.0) } else { font(14.0) };
        ui.painter().text(Pos2::new(rect.left() + 52.0, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
        if let Some(badge) = badge {
            ui.painter().text(rect.right_center() - Vec2::new(16.0, 0.0), Align2::RIGHT_CENTER, badge, font_bold(12.0), fg);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Material 3 switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(52.0, 32.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let p = ui.painter();
        let r = CornerRadius::same(SHAPE_FULL);
        let track = if *on { PRIMARY } else { SURFACE_CONTAINER_HIGHEST };
        p.rect_filled(rect, r, track);
        if !*on {
            p.rect_stroke(rect, r, Stroke::new(2.0, OUTLINE), StrokeKind::Inside);
        }
        let radius = egui::lerp(8.0..=12.0, t) + if response.is_pointer_button_down_on() { 2.0 } else { 0.0 };
        let x = egui::lerp((rect.left() + 16.0)..=(rect.right() - 16.0), t);
        let center = Pos2::new(x, rect.center().y);
        if response.hovered() {
            p.circle_filled(center, 20.0, alpha(if *on { PRIMARY } else { ON_SURFACE }, STATE_HOVER));
        }
        p.circle_filled(center, radius, if *on { ON_PRIMARY } else { OUTLINE });
        if t > 0.5 {
            Icon::Check.paint(ui, Rect::from_center_size(center, Vec2::splat(16.0)), alpha(PRIMARY, (t - 0.5) * 2.0));
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Connected outlined buttons, the selected one tonal with a check mark.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> Response {
    let mut changed = None;
    let inner = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, (v, label)) in options.iter().enumerate() {
            let first = i == 0;
            let last = i + 1 == options.len();
            let corners = CornerRadius {
                nw: if first { SHAPE_FULL } else { 0 },
                sw: if first { SHAPE_FULL } else { 0 },
                ne: if last { SHAPE_FULL } else { 0 },
                se: if last { SHAPE_FULL } else { 0 },
            };
            let selected = *value == *v;
            let mut b = Button::new(*label).kind(if selected { Kind::Tonal } else { Kind::Outlined }).small().corners(corners);
            if selected {
                b = b.icon(Icon::Check);
            }
            if b.ui(ui).clicked() {
                changed = Some(*v);
            }
        }
    });
    let mut response = inner.response;
    if let Some(v) = changed {
        *value = v;
        response.mark_changed();
    }
    response
}

/// Material primary tabs; returns the clicked tab, if any.
pub fn tabs(ui: &mut Ui, selected: usize, labels: &[String]) -> Option<usize> {
    let mut clicked = None;
    let (outer, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 48.0), Sense::hover());
    ui.painter().hline(outer.x_range(), outer.bottom() - 0.5, Stroke::new(1.0, alpha(OUTLINE_VARIANT, 0.8)));
    let mut x = outer.left();
    for (i, label) in labels.iter().enumerate() {
        let active = i == selected;
        let fg = if active { PRIMARY } else { ON_SURFACE_VARIANT };
        let galley = ui.painter().layout_no_wrap(label.clone(), font_bold(14.0), fg);
        let rect = Rect::from_min_size(Pos2::new(x, outer.top()), Vec2::new(galley.size().x + 32.0, 48.0));
        let response = ui.interact(rect, ui.scope_id().with(("tab", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, alpha(fg, STATE_HOVER));
        }
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley.clone(), fg);
        if active {
            let w = galley.size().x;
            let bar = Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 1.5), Vec2::new(w, 3.0));
            ui.painter().rect_filled(bar, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, PRIMARY);
        }
        if response.clicked() {
            clicked = Some(i);
        }
        x = rect.right();
    }
    clicked
}

pub struct SplitResponse {
    pub main: Response,
    pub arrow: Response,
}

/// A filled button with a separate drop-down arrow (Material split button).
pub fn split_button(ui: &mut Ui, label: &str, icon: Icon, enabled: bool) -> SplitResponse {
    // `horizontal` follows the parent's direction: keep the arrow on the right either way.
    let rtl = ui.layout().prefer_right_to_left();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let main = |ui: &mut Ui| {
            Button::filled(label)
                .icon(icon)
                .enabled(enabled)
                .corners(CornerRadius { nw: SHAPE_FULL, sw: SHAPE_FULL, ne: 4, se: 4 })
                .ui(ui)
        };
        let arrow = |ui: &mut Ui| {
            Button::filled("")
                .icon(Icon::ExpandMore)
                .enabled(enabled)
                .corners(CornerRadius { nw: 4, sw: 4, ne: SHAPE_FULL, se: SHAPE_FULL })
                .ui(ui)
                .on_hover_text("Choose the game version")
        };
        if rtl {
            let arrow = arrow(ui);
            SplitResponse { main: main(ui), arrow }
        } else {
            let main = main(ui);
            SplitResponse { main, arrow: arrow(ui) }
        }
    })
    .inner
}

/// Muted secondary text.
pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(egui::RichText::new(text.into()).color(ON_SURFACE_VARIANT).font(font(14.0)))
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(font_bold(28.0)).color(ON_SURFACE));
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(font_bold(18.0)).color(ON_SURFACE));
}

/// Label + control row used by the settings forms.
pub fn field_row(ui: &mut Ui, label: &str, hint: Option<&str>, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(220.0, 40.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(220.0);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(label).font(font(14.0)).color(ON_SURFACE));
            if let Some(hint) = hint {
                ui.label(egui::RichText::new(hint).font(font(12.0)).color(ON_SURFACE_VARIANT));
            }
        });
        add(ui);
    });
}

/// Round avatar: a picture, or the user's initial.
pub fn avatar(ui: &mut Ui, url: Option<String>, name: &str, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    match url {
        Some(url) => {
            egui::Image::from_uri(url).corner_radius(CornerRadius::same(SHAPE_FULL)).paint_at(ui, rect);
        }
        None => {
            ui.painter().circle_filled(rect.center(), size / 2.0, PRIMARY_CONTAINER);
            let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, initial, font_bold(size * 0.45), ON_PRIMARY_CONTAINER);
        }
    }
    response
}

/// Single-line text field in the kit style.
pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(WidgetText::from(hint).color(ON_SURFACE_VARIANT))
            .desired_width(width)
            .align(egui::Align2::LEFT_CENTER)
            .margin(Margin::symmetric(12, 10))
            .font(font(14.0)),
    )
}
