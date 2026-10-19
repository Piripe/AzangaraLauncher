//! Display settings, i.e. what the Direct3D config dialog edits.

use crate::config::Config;

/// Every field is optional: `None` means "leave the current value alone".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplaySettings {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fullscreen: Option<bool>,
    /// Refresh rate in Hz (fullscreen only; the OpenGL build uses 0 = default).
    pub refresh: Option<u32>,
    pub vsync: Option<bool>,
    /// Back buffer colour depth, 16 or 32.
    pub color_bits: Option<u32>,
    /// Texture colour depth, 16 or 32.
    pub texture_bits: Option<u32>,
    pub texture_filter: Option<bool>,
    /// 0.0 – 1.0, default 0.5.
    pub brightness: Option<f32>,
    pub fov: Option<f32>,
    /// Frame rate cap, 0 = unlimited.
    pub max_fps: Option<u32>,
    pub show_fps: Option<bool>,
    /// Direct3D adapter index (Direct3D builds only).
    pub adapter: Option<u32>,
}

impl DisplaySettings {
    /// Pairs of (config variable, value to write) for every field that is set.
    pub fn to_vars(&self) -> Vec<(&'static str, String)> {
        let b = |v: bool| if v { "1" } else { "0" }.to_string();
        let mut vars = Vec::new();
        let mut push = |k, v: Option<String>| {
            if let Some(v) = v {
                vars.push((k, v));
            }
        };
        push("r_width", self.width.map(|v| v.to_string()));
        push("r_height", self.height.map(|v| v.to_string()));
        push("r_fullscreen", self.fullscreen.map(b));
        push("r_refresh", self.refresh.map(|v| v.to_string()));
        push("r_vsync", self.vsync.map(b));
        push("r_pixel_bits", self.color_bits.map(|v| v.to_string()));
        push("r_tex_bits", self.texture_bits.map(|v| v.to_string()));
        push("r_tex_filter", self.texture_filter.map(b));
        push("r_brightness", self.brightness.map(|v| format!("{v:.6}")));
        push("r_fov", self.fov.map(|v| v.to_string()));
        push("com_maxfps", self.max_fps.map(|v| v.to_string()));
        push("r_drawfps", self.show_fps.map(b));
        push("r_d3d_device", self.adapter.map(|v| v.to_string()));
        vars
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, bits) in [("color", self.color_bits), ("texture", self.texture_bits)] {
            if bits.is_some_and(|b| b != 16 && b != 32) {
                return Err(format!("{name} depth must be 16 or 32"));
            }
        }
        if self.brightness.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
            return Err("brightness must be between 0 and 1".into());
        }
        if self.width == Some(0) || self.height == Some(0) {
            return Err("resolution must be non-zero".into());
        }
        Ok(())
    }

    pub fn apply(&self, cfg: &mut Config) -> Result<(), String> {
        self.validate()?;
        for (k, v) in self.to_vars() {
            cfg.set(k, &v)?;
        }
        Ok(())
    }

    /// Reads the current settings from a config.
    pub fn read(cfg: &Config) -> Self {
        let num = |k| cfg.get(k).and_then(|v| v.trim().parse::<f32>().ok());
        let int = |k| num(k).map(|v| v as u32);
        let flag = |k| num(k).map(|v| v != 0.0);
        Self {
            width: int("r_width"),
            height: int("r_height"),
            fullscreen: flag("r_fullscreen"),
            refresh: int("r_refresh"),
            vsync: flag("r_vsync"),
            color_bits: int("r_pixel_bits"),
            texture_bits: int("r_tex_bits"),
            texture_filter: flag("r_tex_filter"),
            brightness: num("r_brightness"),
            fov: num("r_fov"),
            max_fps: int("com_maxfps"),
            show_fps: flag("r_drawfps"),
            adapter: int("r_d3d_device"),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
