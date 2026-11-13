use alacritty_terminal::term::TermMode;
use zui_input::{Key, KeyInput};

pub fn encode(event: &KeyInput, mode: TermMode) -> Option<Vec<u8>> {
    if !event.pressed || event.modifiers.super_key {
        return None;
    }
    let m = event.modifiers;
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let cursor = |suffix: char| {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{suffix}")
        } else if mode.contains(TermMode::APP_CURSOR) {
            format!("\x1bO{suffix}")
        } else {
            format!("\x1b[{suffix}")
        }
    };
    let tilde = |n| {
        if modifier > 1 {
            format!("\x1b[{n};{modifier}~")
        } else {
            format!("\x1b[{n}~")
        }
    };
    let text = match &event.key {
        Key::Enter => "\r".into(),
        Key::Escape => "\x1b".into(),
        Key::Tab => if m.shift { "\x1b[Z" } else { "\t" }.into(),
        Key::Backspace => if m.control { "\x08" } else { "\x7f" }.into(),
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::Function(n @ 1..=4) => {
            let suffix = char::from(b'P' + n - 1);
            if modifier > 1 {
                format!("\x1b[1;{modifier}{suffix}")
            } else {
                format!("\x1bO{suffix}")
            }
        }
        Key::Function(n @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)]),
        Key::Character(s) if m.control => {
            let c = s.chars().next()?.to_ascii_lowercase();
            let byte = match c {
                'a'..='z' => c as u8 - b'a' + 1,
                ' ' | '@' | '2' => 0,
                '[' | '3' => 27,
                '\\' | '4' => 28,
                ']' | '5' => 29,
                '^' | '6' => 30,
                '_' | '7' => 31,
                '?' | '8' => 127,
                _ => return None,
            };
            String::from(char::from(byte))
        }
        Key::Space if m.control => "\0".into(),
        Key::Space => " ".into(),
        Key::Character(s) => event.text.clone().unwrap_or_else(|| s.clone()),
        _ => return None,
    };
    let mut bytes = text.into_bytes();
    if m.alt
        && matches!(
            event.key,
            Key::Character(_) | Key::Space | Key::Enter | Key::Backspace
        )
    {
        bytes.insert(0, 0x1b);
    }
    Some(bytes)
}

pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Strip escape characters so pasted data cannot close bracketed-paste mode early.
    let text = text.replace('\x1b', "").replace("\r\n", "\n");
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.replace('\n', "\r").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_input::Modifiers;
    fn key(key: Key, modifiers: Modifiers) -> KeyInput {
        KeyInput {
            key,
            modifiers,
            pressed: true,
            repeat: false,
            text: None,
        }
    }
    #[test]
    fn encodes_control_and_application_keys() {
        assert_eq!(
            encode(
                &key(
                    Key::Character("c".into()),
                    Modifiers {
                        control: true,
                        ..Default::default()
                    }
                ),
                TermMode::empty()
            )
            .unwrap(),
            b"\x03"
        );
        assert_eq!(
            encode(
                &key(Key::ArrowUp, Modifiers::default()),
                TermMode::APP_CURSOR
            )
            .unwrap(),
            b"\x1bOA"
        );
        assert_eq!(
            encode(
                &key(
                    Key::ArrowLeft,
                    Modifiers {
                        control: true,
                        ..Default::default()
                    }
                ),
                TermMode::empty()
            )
            .unwrap(),
            b"\x1b[1;5D"
        );
        assert_eq!(
            encode(
                &key(Key::Function(12), Modifiers::default()),
                TermMode::empty()
            )
            .unwrap(),
            b"\x1b[24~"
        );
    }
    #[test]
    fn paste_preserves_unicode_and_cannot_inject_terminator() {
        assert_eq!(
            paste("你好\r\n\x1b[201~", true),
            "\x1b[200~你好\n[201~\x1b[201~".as_bytes()
        );
    }
}
