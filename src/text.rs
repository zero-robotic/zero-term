use std::{collections::HashMap, path::PathBuf, sync::Arc};
use zui_api::text::{TextAlign, TextLayout, TextRequest, TextService, TextStyle, TextWidth};
use zui_app::ApplicationError;
use zui_paint::Painter;
use zui_text::{ParleyText, TextLimits};
use zui_types::{Affine, Color};

pub struct TextPainter {
    service: ParleyText,
    style: TextStyle,
    cache: HashMap<String, TextLayout>,
    wide_cache: HashMap<String, TextLayout>,
    baseline: f32,
    pub cell_width: f64,
    pub cell_height: f64,
}
impl TextPainter {
    pub fn new() -> Result<Self, ApplicationError> {
        let mut service = ParleyText::new(TextLimits {
            max_font_bytes: 128 * 1024 * 1024,
            max_font_faces: 128,
            ..Default::default()
        });
        let override_font = std::env::var_os("ZERO_TERM_FONT").map(PathBuf::from);
        let paths: Vec<PathBuf> = override_font
            .clone()
            .into_iter()
            .chain(
                [
                    "/System/Library/Fonts/Menlo.ttc",
                    "/System/Library/Fonts/SFNSMono.ttf",
                    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
                    "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
                    "C:/Windows/Fonts/consola.ttf",
                ]
                .into_iter()
                .map(PathBuf::from),
            )
            .collect();
        let path = paths
            .iter()
            .find(|p| p.is_file())
            .ok_or("No monospace font found; set ZERO_TERM_FONT to a TTF/OTF/TTC file")?;
        if override_font.as_ref().is_some_and(|p| !p.is_file()) {
            return Err("ZERO_TERM_FONT does not exist".into());
        }
        let mut families = service.register_font(Arc::from(std::fs::read(path)?))?;
        let fallback = std::env::var_os("ZERO_TERM_FALLBACK_FONT")
            .map(PathBuf::from)
            .or_else(|| {
                [
                    "/System/Library/Fonts/Hiragino Sans GB.ttc",
                    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                    "C:/Windows/Fonts/msyh.ttc",
                ]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
            });
        if let Some(path) = fallback {
            families.extend(service.register_font(Arc::from(std::fs::read(path)?))?);
        }
        let style = TextStyle {
            families,
            font_size: 14.0,
            line_height: 1.4,
            align: TextAlign::Start,
        };
        let measure = service.layout(TextRequest {
            text: "M",
            style: &style,
            width: TextWidth::Unbounded,
        })?;
        Ok(Self {
            service,
            style: style.clone(),
            cache: HashMap::new(),
            wide_cache: HashMap::new(),
            baseline: measure.first_baseline.unwrap_or(style.font_size),
            cell_width: measure.size.width,
            cell_height: measure.size.height.ceil().max(20.0),
        })
    }
    /// Wide characters occupy two terminal columns. Match the fallback font's
    /// advance to those columns with uniform font scaling, preserving its shape.
    pub fn paint_wide_cell(
        &mut self,
        painter: &mut Painter<'_>,
        text: &str,
        x: f64,
        y: f64,
        color: Color,
    ) -> Result<(), ApplicationError> {
        if !self.wide_cache.contains_key(text) {
            if self.wide_cache.len() >= 4096 {
                self.wide_cache.clear();
            }
            let layout = self.service.layout(TextRequest {
                text,
                style: &self.style,
                width: TextWidth::Unbounded,
            })?;
            let mut style = self.style.clone();
            if layout.size.width > 0.0 {
                style.font_size *= (2.0 * self.cell_width / layout.size.width) as f32;
            }
            let layout = self.service.layout(TextRequest {
                text,
                style: &style,
                width: TextWidth::Unbounded,
            })?;
            self.wide_cache.insert(text.to_owned(), layout);
        }
        let layout = &self.wide_cache[text];
        let baseline_offset =
            f64::from(self.baseline - layout.first_baseline.unwrap_or(self.baseline));
        layout.paint_at(painter, x, y + baseline_offset, color);
        Ok(())
    }
    pub fn paint(
        &mut self,
        painter: &mut Painter<'_>,
        text: &str,
        x: f64,
        y: f64,
        color: Color,
    ) -> Result<(), ApplicationError> {
        if !self.cache.contains_key(text) {
            if self.cache.len() >= 4096 {
                self.cache.clear();
            }
            let layout = self.service.layout(TextRequest {
                text,
                style: &self.style,
                width: TextWidth::Unbounded,
            })?;
            self.cache.insert(text.to_owned(), layout);
        }
        self.cache[text].paint_at(painter, x, y, color);
        Ok(())
    }
}
trait PaintAt {
    fn paint_at(&self, painter: &mut Painter<'_>, x: f64, y: f64, color: Color);
}
impl PaintAt for TextLayout {
    fn paint_at(&self, painter: &mut Painter<'_>, x: f64, y: f64, color: Color) {
        painter.with_transform(Affine::translate(x, y), |p| self.paint(p, color));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cjk_advance_matches_two_terminal_columns() {
        let mut text = TextPainter::new().unwrap();
        let natural = text
            .service
            .layout(TextRequest {
                text: "中",
                style: &text.style,
                width: TextWidth::Unbounded,
            })
            .unwrap();
        let mut scene = zui_paint::Scene::new();
        text.paint_wide_cell(&mut scene.painter(), "中", 0.0, 0.0, Color::WHITE)
            .unwrap();
        let fitted = &text.wide_cache["中"];
        assert!((fitted.size.width - 2.0 * text.cell_width).abs() < 0.1);
        assert_eq!(fitted.missing_glyph_count(), 0);
        eprintln!(
            "Latin cell: {:.3}; natural CJK: {:.3}; fitted CJK: {:.3}",
            text.cell_width, natural.size.width, fitted.size.width
        );
    }
}
