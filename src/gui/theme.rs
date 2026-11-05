//! The AzangaraMods website look: its `globals.css` tokens (converted from
//! oklab to sRGB), the Inter font and the tiled background.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Mesh, Pos2, Rect, Shadow, Stroke,
    TextStyle, Vec2,
};

pub const PRIMARY: Color32 = Color32::from_rgb(0xf4, 0xb1, 0x6c);
pub const ON_PRIMARY: Color32 = Color32::from_rgb(0x38, 0x18, 0x00);
pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(0x64, 0x3a, 0x00);
pub const ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(0xff, 0xde, 0xbc);
pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(0x50, 0x3c, 0x55);
pub const ON_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(0xed, 0xdf, 0xf1);
pub const SURFACE: Color32 = Color32::from_rgb(0x17, 0x12, 0x18);
pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(0x21, 0x1a, 0x23);
pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(0x2b, 0x23, 0x2d);
pub const SURFACE_CONTAINER_HIGHEST: Color32 = Color32::from_rgb(0x36, 0x2d, 0x38);
pub const ON_SURFACE: Color32 = Color32::from_rgb(0xed, 0xed, 0xed);
pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(0xc9, 0xc1, 0xcb);
pub const OUTLINE: Color32 = Color32::from_rgb(0x85, 0x7d, 0x88);
pub const OUTLINE_VARIANT: Color32 = Color32::from_rgb(0x4d, 0x44, 0x50);
pub const INVERSE_SURFACE: Color32 = Color32::from_rgb(0xe6, 0xe1, 0xe6);
pub const INVERSE_ON_SURFACE: Color32 = Color32::from_rgb(0x32, 0x2b, 0x33);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0xb4, 0xab);
const BODY_CENTER: Color32 = Color32::from_rgb(0x21, 0x1b, 0x21);
const BODY_EDGE: Color32 = Color32::from_rgb(0x16, 0x13, 0x16);

pub const STATE_HOVER: f32 = 0.08;
pub const STATE_PRESSED: f32 = 0.12;

pub const SHAPE_SM: u8 = 8;
pub const SHAPE_MD: u8 = 12;
pub const SHAPE_LG: u8 = 16;
pub const SHAPE_FULL: u8 = 255;

/// `rgb(from color r g b / a)`
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0).round() as u8)
}

/// A state layer of `content` over `container` (Material hover/pressed).
pub fn layer(container: Color32, content: Color32, amount: f32) -> Color32 {
    if container.a() == 0 {
        return alpha(content, amount);
    }
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
    Color32::from_rgba_unmultiplied(mix(container.r(), content.r()), mix(container.g(), content.g()), mix(container.b(), content.b()), container.a())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, semibold())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf")).into(),
    );
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "inter".into());
    fonts.families.insert(semibold(), [vec!["inter-semibold".into()], fallbacks].concat());
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, font(12.0)),
            (TextStyle::Body, font(14.0)),
            (TextStyle::Button, font_bold(14.0)),
            (TextStyle::Heading, font_bold(28.0)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.combo_height = 300.0;
        style.spacing.slider_width = 220.0;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(ON_SURFACE);
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = SURFACE_CONTAINER_HIGH;
        v.window_stroke = Stroke::new(1.0, alpha(OUTLINE_VARIANT, 0.5));
        v.window_corner_radius = CornerRadius::same(28);
        v.window_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: alpha(Color32::BLACK, 0.4) };
        v.popup_shadow = Shadow { offset: [0, 2], blur: 8, spread: 0, color: alpha(Color32::BLACK, 0.35) };
        v.menu_corner_radius = CornerRadius::same(SHAPE_SM);
        v.extreme_bg_color = alpha(SURFACE_CONTAINER_HIGHEST, 0.6);
        v.faint_bg_color = SURFACE_CONTAINER;
        v.hyperlink_color = PRIMARY;
        v.error_fg_color = ERROR;
        v.warn_fg_color = PRIMARY;
        v.selection.bg_fill = alpha(PRIMARY, 0.35);
        v.selection.stroke = Stroke::new(1.0, PRIMARY);
        v.text_cursor.stroke = Stroke::new(2.0, PRIMARY);
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;

        let radius = CornerRadius::same(SHAPE_SM);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE_CONTAINER;
        w.noninteractive.weak_bg_fill = SURFACE_CONTAINER;
        w.noninteractive.bg_stroke = Stroke::new(1.0, alpha(OUTLINE_VARIANT, 0.6));
        w.noninteractive.fg_stroke = Stroke::new(1.0, ON_SURFACE_VARIANT);
        w.noninteractive.corner_radius = radius;
        for (state, fill, stroke) in [
            (&mut w.inactive, alpha(SURFACE_CONTAINER_HIGHEST, 0.6), OUTLINE_VARIANT),
            (&mut w.hovered, layer(SURFACE_CONTAINER_HIGHEST, ON_SURFACE, STATE_HOVER), OUTLINE),
            (&mut w.active, layer(SURFACE_CONTAINER_HIGHEST, ON_SURFACE, STATE_PRESSED), PRIMARY),
            (&mut w.open, SURFACE_CONTAINER_HIGHEST, OUTLINE),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, stroke);
            state.fg_stroke = Stroke::new(1.5, ON_SURFACE);
            state.corner_radius = radius;
            state.expansion = 0.0;
        }
        // Slider handles and checked boxes use the primary colour.
        w.active.bg_fill = PRIMARY;
    });
}

/// Paints `body`'s radial gradient and the faint tiled silhouettes.
pub fn paint_background(ui: &egui::Ui, rect: Rect) {
    let painter = ui.painter();
    let center = rect.center();
    let radius = rect.size() * 0.5 * std::f32::consts::SQRT_2;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, BODY_CENTER);
    const STEPS: u32 = 48;
    for i in 0..=STEPS {
        let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(Pos2::new(center.x + a.cos() * radius.x, center.y + a.sin() * radius.y), BODY_EDGE);
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    painter.add(mesh);

    // `background: url(background.svg) repeat center 0% / 1536px; opacity: .3`
    let tile = Vec2::new(1536.0, 1536.0 * 278.8 / 513.0);
    let image = egui::Image::from_bytes("bytes://background.svg", include_bytes!("../../assets/background.svg"))
        .tint(alpha(Color32::BLACK, 0.3));
    let start_x = center.x - tile.x * 0.5 - ((center.x - tile.x * 0.5 - rect.left()) / tile.x).ceil() * tile.x;
    let mut y = rect.top();
    while y < rect.bottom() {
        let mut x = start_x;
        while x < rect.right() {
            image.paint_at(ui, Rect::from_min_size(Pos2::new(x, y), tile));
            x += tile.x;
        }
        y += tile.y;
    }
}
