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
    process: crate::process::ForegroundProcess,
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
        let mut process = crate::process::ForegroundProcess::new(&pty)?;
        let title = process.poll().unwrap_or(title);
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
            process,
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
                // OSC titles often contain usernames/directories and must not replace
                // the foreground executable name shown in the tab.
                Event::Title(_) | Event::ResetTitle => {}
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
        if !self.exited {
            if let Some(title) = self.process.poll() {
                if title != self.title {
                    self.title = title;
                    changed = true;
                }
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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn title_tracks_foreground_job_and_returns_to_shell() {
        let size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 9,
            cell_height: 20,
        };
        let mut session = Session::new(
            "Shell".into(),
            size,
            Some(tty::Shell::new(
                "/bin/bash".into(),
                vec!["--noprofile".into(), "--norc".into(), "-i".into()],
            )),
        )
        .unwrap();
        fn wait_for(session: &mut Session, expected: &str) {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                session.poll();
                if session.title == expected {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            panic!("expected title {expected}, got {}", session.title);
        }
        wait_for(&mut session, "bash");
        session.write(b"printf '\\033]0;username@host:directory\\007'; sleep 2\r".to_vec());
        wait_for(&mut session, "sleep");
        wait_for(&mut session, "bash");
        session.write(b"exit\r".to_vec());
    }
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
