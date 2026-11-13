//! Foreground process titles, independent of shell-supplied OSC window titles.
use std::time::{Duration, Instant};

pub fn short_name(name: &str) -> String {
    let name = name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim_start_matches('-');
    let name = name.strip_suffix(".exe").unwrap_or(name);
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > 12 {
        format!("{}…", clean.chars().take(11).collect::<String>())
    } else {
        clean
    }
}

pub struct ForegroundProcess {
    #[cfg(unix)]
    master: std::fs::File,
    #[cfg(unix)]
    shell_pid: i32,
    checked: Option<Instant>,
}
impl ForegroundProcess {
    pub fn new(pty: &alacritty_terminal::tty::Pty) -> std::io::Result<Self> {
        #[cfg(not(unix))]
        let _ = pty;
        Ok(Self {
            #[cfg(unix)]
            master: pty.file().try_clone()?,
            #[cfg(unix)]
            shell_pid: pty.child().id() as i32,
            checked: None,
        })
    }
    pub fn poll(&mut self) -> Option<String> {
        let now = Instant::now();
        if self
            .checked
            .is_some_and(|last| now.duration_since(last) < Duration::from_millis(250))
        {
            return None;
        }
        self.checked = Some(now);
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: the owned master descriptor remains valid throughout this call.
            let group = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
            let pid = if group > 0 { group } else { self.shell_pid };
            process_name(pid)
                .map(|name| short_name(&name))
                .filter(|name| !name.is_empty())
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn process_name(pid: i32) -> Option<String> {
    let mut buffer = [0_u8; 4096];
    // SAFETY: buffer is writable for its declared capacity; libproc bounds its writes.
    let count = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if count <= 0 {
        return None;
    }
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}
#[cfg(target_os = "linux")]
fn process_name(pid: i32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(|| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok())
}
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn process_name(_pid: i32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_are_short_process_names() {
        assert_eq!(short_name("/bin/zsh"), "zsh");
        assert_eq!(short_name("-bash"), "bash");
        assert_eq!(short_name("C:\\Tools\\python.exe"), "python");
        assert_eq!(short_name("very-long-process-name"), "very-long-p…");
        assert_eq!(short_name("中文进程"), "中文进程");
    }
}
