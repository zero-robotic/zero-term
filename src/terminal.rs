use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    event_loop::{EventLoop, EventLoopSender, Msg},
    grid::Scroll,
    sync::FairMutex,
    term::{test::TermSize, Config},
    tty, Term,
};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

#[derive(Clone)]
pub struct Proxy {
    tx: mpsc::Sender<Event>,
    dirty: Arc<AtomicBool>,
}
impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        self.dirty.store(true, Ordering::Release);
        // Output notifications are coalesced so a busy shell cannot fill the UI queue.
        if !matches!(
            event,
            Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange
        ) {
            let _ = self.tx.send(event);
        }
    }
}

pub struct Session {
    pub term: Arc<FairMutex<Term<Proxy>>>,
    pub title: String,
    pub exited: bool,
    pub size: WindowSize,
    sender: EventLoopSender,
    events: mpsc::Receiver<Event>,
    dirty: Arc<AtomicBool>,
}
impl Session {
    pub fn new(title: String, size: WindowSize, shell: Option<tty::Shell>) -> io::Result<Self> {
        let (tx, events) = mpsc::channel();
        let dirty = Arc::new(AtomicBool::new(true));
        let proxy = Proxy {
            tx,
            dirty: dirty.clone(),
        };
        let config = Config {
            scrolling_history: 10_000,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &TermSize::new(size.num_cols.into(), size.num_lines.into()),
            proxy.clone(),
        )));
        let cwd = std::env::current_dir()
            .ok()
            .filter(|path| path.parent().is_some())
            .or_else(|| std::env::var_os("HOME").map(Into::into));
        let mut options = tty::Options {
            shell,
            working_directory: cwd,
            ..Default::default()
        };
        options.env.insert("TERM".into(), "xterm-256color".into());
        options.env.insert("COLORTERM".into(), "truecolor".into());
        let pty = tty::new(&options, size, 0)?;
        let event_loop = EventLoop::new(term.clone(), proxy, pty, true, false)?;
        let sender = event_loop.channel();
        // Detach: Shutdown drops the event loop and PTY on its own thread, reaping the child.
        drop(event_loop.spawn());
        Ok(Self {
            term,
            title,
            exited: false,
            size,
            sender,
            events,
            dirty,
        })
    }
    pub fn write(&self, bytes: Vec<u8>) {
        if !self.exited {
            let _ = self.sender.send(Msg::Input(bytes.into()));
        }
    }
    pub fn input(&self, bytes: Vec<u8>) {
        let mut term = self.term.lock();
        term.scroll_display(Scroll::Bottom);
        term.selection = None;
        drop(term);
        self.write(bytes);
    }
    pub fn resize(&mut self, size: WindowSize) {
        if self.size.num_cols == size.num_cols && self.size.num_lines == size.num_lines {
            return;
        }
        self.size = size;
        self.term
            .lock()
            .resize(TermSize::new(size.num_cols.into(), size.num_lines.into()));
        let _ = self.sender.send(Msg::Resize(size));
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = self.dirty.swap(false, Ordering::AcqRel);
        // Bound per-frame work if a program emits many OSC title changes.
        for _ in 0..512 {
            let Ok(event) = self.events.try_recv() else {
                break;
            };
            changed = true;
            match event {
                Event::Title(title) => self.title = title,
                Event::ResetTitle => self.title = "Shell".into(),
                Event::PtyWrite(text) => self.write(text.into_bytes()),
                Event::TextAreaSizeRequest(format) => self.write(format(self.size).into_bytes()),
                Event::ColorRequest(index, format) => {
                    let color = self.term.lock().colors()[index]
                        .unwrap_or_else(|| crate::palette::indexed(index));
                    self.write(format(color).into_bytes());
                }
                // OSC 52 reads/writes are deliberately not connected to the system clipboard.
                // Clipboard access is driven by explicit user copy/paste shortcuts.
                Event::ClipboardLoad(_, format) => self.write(format("").into_bytes()),
                Event::Exit | Event::ChildExit(_) => self.exited = true,
                _ => {}
            }
        }
        changed
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn real_pty_runs_shell_and_resizes() {
        let size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 9,
            cell_height: 20,
        };
        let shell = tty::Shell::new("/bin/sh".into(), vec![]);
        let mut session = Session::new("test".into(), size, Some(shell)).unwrap();
        session.resize(WindowSize {
            num_cols: 100,
            num_lines: 30,
            ..size
        });
        session.write(b"printf '\\033[31mPTY_OK\\033[0m\\n'; stty size; exit\r".to_vec());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !session.exited && Instant::now() < deadline {
            session.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(session.exited, "shell must terminate");
        let term = session.term.lock();
        let text: String = term.grid().display_iter().map(|c| c.c).collect();
        assert!(text.contains("PTY_OK"), "{text}");
        assert!(
            text.contains("30 100"),
            "PTY must receive new dimensions: {text}"
        );
    }
}
