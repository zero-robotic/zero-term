use std::sync::Arc;

use alacritty_terminal::term::cell::Flags;
use parking_lot::RwLock;

use druid::kurbo::BezPath;
use druid::piet::{FontFamily, ImageFormat, InterpolationMode, Text, TextLayoutBuilder};
use druid::widget::prelude::*;
use druid::{
    Affine, AppLauncher, Color, Data, Env, FontDescriptor, LocalizedString, Point, Rect, TextLayout,
    WindowDesc,
};

use crate::terminal::TermId;
use crate::raw::RawTerminal;

pub struct TerminalWidget {
    term_id: TermId,
    raw: Arc<RwLock<RawTerminal>>,
    is_focused: bool,
}

impl TerminalWidget {
    pub fn new(term_id: TermId, raw: Arc<RwLock<RawTerminal>>, is_focused: bool) -> Self {
       TerminalWidget {
            term_id,
            raw,
            is_focused
       }
    }
}

impl<T: Data> Widget<T> for TerminalWidget {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut T, _evn: &Env) {}

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &T,
        _env: &Env,
    ) {
    }

    fn update(&mut self, _ctx: &mut UpdateCtx, _old_data: &T, _data: &T, _env: &Env) {}

    fn layout(
        &mut self,
        _layout_ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        _data: &T,
        _env: &Env,
    ) -> Size {
        if bc.is_width_bounded() && bc.is_height_bounded() {
            bc.max()
        } else {
            let size = Size::new(100.0, 100.0);
            bc.constrain(size)
        }
    }

    fn paint(&mut self, ctx: &mut PaintCtx, _data: &T, env: &Env) {
        let origin = Point::new(2.0, 0.0);

        let raw = self.raw.read();
        let terminal = &raw.terminal.lock();
        let content = terminal.renderable_content();
        //if let Some(selection) = content.selection.as_ref() {
            
        //}
        
        let mut text_layout = TextLayout::<String>::new();
        for item in content.display_iter {
            let point = item.point;
            let cell = item.cell;
            let inverse = cell.flags.contains(Flags::INVERSE);

            let x = point.column.0 as f64 * 14.0;
            let y = (point.line.0 as f64 + content.display_offset as f64) * 14.0;

            //text_layout.set_text(cell.c.to_string());
            text_layout.set_text(cell.c.to_string());
            text_layout.rebuild_if_needed(ctx.text(), env);
            ctx.with_save(|ctx| {
                text_layout.draw(ctx, (x, y));
            })
        }
    }
}