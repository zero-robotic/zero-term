mod app;
mod input;
mod palette;
mod process;
mod terminal;
mod text;

use std::time::Duration;
use zui_app::{ApplicationError, Backend};
use zui_platform::WindowConfig;
use zui_types::Size;

fn main() -> Result<(), ApplicationError> {
    let mut smoke = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke-ms" => {
                smoke = Some(Duration::from_millis(
                    args.next().ok_or("missing --smoke-ms value")?.parse()?,
                ))
            }
            "--help" | "-h" => {
                println!("zero-term [--smoke-ms N]\nFont: ZERO_TERM_FONT; CJK fallback: ZERO_TERM_FALLBACK_FONT\nNew/close tab: Cmd+T/W or Ctrl+Shift+T/W\nSwitch: Ctrl+Tab / Ctrl+Shift+Tab; direct: Cmd+1..9 or Ctrl+Shift+1..9\nCopy/paste: Cmd+C/V or Ctrl+Shift+C/V; scroll: wheel or Shift+PageUp/PageDown");
                return Ok(());
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    let app = app::TerminalApp::new(smoke)?;
    let report = zui_app::run(
        WindowConfig {
            title: "Zero Term".into(),
            size: Size::new(1000.0, 680.0),
            ..Default::default()
        },
        Backend::Gpu,
        app,
    )?;
    if smoke.is_some() {
        println!(
            "Smoke test: {} events, {} frames",
            report.events, report.redraws
        );
        if report.redraws == 0 {
            return Err("window did not render".into());
        }
    }
    Ok(())
}
