use std::error::Error;
#[cfg(not(windows))]
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;

use alacritty_terminal::event::{WindowSize};
use alacritty_terminal::event_loop::{EventLoop as PtyEventLoop, Msg, Notifier};
use alacritty_terminal::tty::{self, Options as PtyOptions};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::{term::Config, event::EventListener, term::test::TermSize, Term};
use vte::ansi;

use crate::terminal::TermId;

#[derive(Copy, Clone)]
pub struct EventProxy {
    term_id: TermId
}

impl EventListener for EventProxy {
    fn send_event(&self, event: alacritty_terminal::event::Event) {
        match event {
            _ => (),
        }
    }
}

pub struct RawTerminal {
    pub parser: ansi::Processor,
    pub terminal: Arc<FairMutex<Term<EventProxy>>>,
    pub scroll_delta: f64,

    notifier: Notifier,
    #[cfg(not(Windows))]
    master_fd: RawFd,
    #[cfg(not(windows))]
    shell_pid: u32,
}

impl RawTerminal {
    pub fn new(term_id: TermId) -> Result<Self, Box<dyn Error>> {
        let event_proxy = EventProxy {
            term_id,
        };

        // Create the terminal.
        //
        // This object contains all of the state about what's being displayed. It's
        // wrapped in a clonable mutex since both the I/O loop and display need to
        // access it.
        let config = Config::default();
        let size = TermSize::new(50, 30);
        let term = Term::new(config, &size, event_proxy);
        let terminal = Arc::new(FairMutex::new(term));
        let parser = ansi::Processor::new();

        // Create the PTY.
        //
        // The PTY forks a process to run the shell on the slave side of the
        // pseudoterminal. A file descriptor for the master side is retained for
        // reading/writing to the shell.
        let pty_config: PtyOptions = PtyOptions {
            shell: None,
            working_directory: None,
            hold: false
        };

        let window_size = WindowSize {
            num_lines: 30,
            num_cols: 50,
            cell_width: 12,
            cell_height: 12
        };
        let pty = tty::new(&pty_config, window_size, term_id.0)?;

        #[cfg(not(windows))]
        let master_fd = pty.file().as_raw_fd();
        #[cfg(not(windows))]
        let shell_pid = pty.child().id();

        // Create the pseudoterminal I/O loop.
        //
        // PTY I/O is ran on another thread as to not occupy cycles used by the
        // renderer and input processing. Note that access to the terminal state is
        // synchronized since the I/O loop updates the state, and the display
        // consumes it periodically.
        let event_loop = PtyEventLoop::new(
            Arc::clone(&terminal),
            event_proxy,
            pty,
            pty_config.hold,
            false
        )?;

        // The event loop channel allows write requests from the event processor
        // to be sent to the pty loop and ultimately written to the pty.
        let loop_tx = event_loop.channel();

        // Kick off the I/O thread.
        let _io_thread = event_loop.spawn();

        // Start cursor blinking, is case `Focused` isn't sent on startup.
        // TODO(sgk)



        Ok(Self {
            parser,
            terminal,
            scroll_delta: 0.0,
            #[cfg(not(windows))]
            master_fd,
            #[cfg(not(windows))]
            shell_pid,
            notifier: Notifier(loop_tx),
        })
    }

    //pub fn update_content(&mut self, content: Vec<u8>) {
    //    let mut terminal = self.terminal.lock();
    //    for byte in content {
    //        self.parser.advance(&mut terminal, byte);
    //    }
    //}
}