//! Translation between egui key events and the platform-neutral
//! [`rc_core::keymap::Shortcut`] tokens.

use rc_core::keymap::Shortcut;

/// Canonical token for an egui key, or `None` for keys that must not be
/// normalised (raw text keys are handled by the text input widgets).
pub fn key_token(key: egui::Key) -> Option<&'static str> {
    use egui::Key::*;
    Some(match key {
        ArrowDown => "DOWN",
        ArrowLeft => "LEFT",
        ArrowRight => "RIGHT",
        ArrowUp => "UP",
        Escape => "ESC",
        Tab => "TAB",
        Backspace => "BACKSPACE",
        Enter => "ENTER",
        Space => "SPACE",
        Insert => "INSERT",
        Delete => "DELETE",
        Home => "HOME",
        End => "END",
        PageUp => "PGUP",
        PageDown => "PGDN",
        Minus => "MINUS",
        Plus => "NUMPLUS",
        Equals => "NUMPLUS",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        A => "A",
        B => "B",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        H => "H",
        I => "I",
        J => "J",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        O => "O",
        P => "P",
        Q => "Q",
        R => "R",
        S => "S",
        T => "T",
        U => "U",
        V => "V",
        W => "W",
        X => "X",
        Y => "Y",
        Z => "Z",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        _ => return None,
    })
}

/// Build a [`Shortcut`] from an egui key press.
pub fn shortcut_from_egui(key: egui::Key, modifiers: egui::Modifiers) -> Option<Shortcut> {
    let token = key_token(key)?;
    let mut sc = Shortcut::new(token);
    sc.ctrl = modifiers.ctrl || modifiers.command;
    sc.alt = modifiers.alt;
    sc.shift = modifiers.shift;
    sc.meta = modifiers.mac_cmd;
    Some(sc)
}

/// True when the key event should be treated as text input rather than a
/// command (letters/digits without modifiers).
pub fn is_text_key(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    if modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd {
        return false;
    }
    matches!(
        key,
        egui::Key::A
            | egui::Key::B
            | egui::Key::C
            | egui::Key::D
            | egui::Key::E
            | egui::Key::F
            | egui::Key::G
            | egui::Key::H
            | egui::Key::I
            | egui::Key::J
            | egui::Key::K
            | egui::Key::L
            | egui::Key::M
            | egui::Key::N
            | egui::Key::O
            | egui::Key::P
            | egui::Key::Q
            | egui::Key::R
            | egui::Key::S
            | egui::Key::T
            | egui::Key::U
            | egui::Key::V
            | egui::Key::W
            | egui::Key::X
            | egui::Key::Y
            | egui::Key::Z
            | egui::Key::Num0
            | egui::Key::Num1
            | egui::Key::Num2
            | egui::Key::Num3
            | egui::Key::Num4
            | egui::Key::Num5
            | egui::Key::Num6
            | egui::Key::Num7
            | egui::Key::Num8
            | egui::Key::Num9
            | egui::Key::Period
            | egui::Key::Minus
    )
}

/// Format a shortcut for display in menus (`Ctrl+F3`).
pub fn display_shortcut(sc: &Shortcut) -> String {
    let mut parts: Vec<String> = Vec::new();
    if sc.ctrl {
        parts.push("Ctrl".to_string());
    }
    if sc.shift {
        parts.push("Shift".to_string());
    }
    if sc.alt {
        parts.push("Alt".to_string());
    }
    if sc.meta {
        parts.push("Meta".to_string());
    }
    let key = match sc.key.as_str() {
        "DELETE" => "Del".to_string(),
        "INSERT" => "Ins".to_string(),
        "ESC" => "Esc".to_string(),
        "PGUP" => "PgUp".to_string(),
        "PGDN" => "PgDn".to_string(),
        "SPACE" => "Space".to_string(),
        "BACKSPACE" => "Backspace".to_string(),
        "ENTER" => "Enter".to_string(),
        "NUMPLUS" => "Num+".to_string(),
        "NUMMINUS" => "Num-".to_string(),
        "NUMMULTIPLY" => "Num*".to_string(),
        "NUMDIVIDE" => "Num/".to_string(),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c.to_uppercase().to_string(),
                _ => title_case(other),
            }
        }
    };
    parts.push(key);
    parts.join("+")
}

fn title_case(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_keys_map_to_tokens() {
        assert_eq!(key_token(egui::Key::F3), Some("F3"));
        assert_eq!(key_token(egui::Key::F12), Some("F12"));
        assert_eq!(key_token(egui::Key::Delete), Some("DELETE"));
        assert_eq!(key_token(egui::Key::PageUp), Some("PGUP"));
        assert_eq!(key_token(egui::Key::A), Some("A"));
        assert_eq!(key_token(egui::Key::Num5), Some("5"));
        assert_eq!(key_token(egui::Key::Colon), None);
    }

    #[test]
    fn modifiers_become_a_shortcut() {
        let m = egui::Modifiers {
            ctrl: true,
            shift: true,
            alt: false,
            mac_cmd: false,
            command: false,
        };
        let sc = shortcut_from_egui(egui::Key::F5, m).unwrap();
        assert!(sc.ctrl && sc.shift && !sc.alt);
        assert_eq!(sc.to_string(), "ctrl+shift+f5");
        let plain = shortcut_from_egui(egui::Key::F7, egui::Modifiers::NONE).unwrap();
        assert_eq!(plain.to_string(), "f7");
        // `command` (macOS cmd) is folded onto ctrl.
        let cmd = egui::Modifiers {
            command: true,
            ..egui::Modifiers::NONE
        };
        assert!(shortcut_from_egui(egui::Key::T, cmd).unwrap().ctrl);
        assert!(shortcut_from_egui(egui::Key::Colon, egui::Modifiers::NONE).is_none());
    }

    #[test]
    fn text_keys_are_detected() {
        assert!(is_text_key(egui::Key::A, egui::Modifiers::NONE));
        assert!(is_text_key(egui::Key::Num3, egui::Modifiers::NONE));
        assert!(!is_text_key(egui::Key::F3, egui::Modifiers::NONE));
        let ctrl = egui::Modifiers {
            ctrl: true,
            ..egui::Modifiers::NONE
        };
        assert!(!is_text_key(egui::Key::A, ctrl));
        assert!(!is_text_key(egui::Key::Escape, egui::Modifiers::NONE));
    }

    #[test]
    fn shortcuts_render_for_menus() {
        let sc = rc_core::keymap::Shortcut::parse("ctrl+shift+f5").unwrap();
        assert_eq!(display_shortcut(&sc), "Ctrl+Shift+F5");
        let del = rc_core::keymap::Shortcut::parse("delete").unwrap();
        assert_eq!(display_shortcut(&del), "Del");
        let letter = rc_core::keymap::Shortcut::new("a");
        assert_eq!(display_shortcut(&letter), "A");
        let num = rc_core::keymap::Shortcut::parse("ctrl+numadd").unwrap();
        assert_eq!(display_shortcut(&num), "Ctrl+Num+");
        assert_eq!(
            display_shortcut(&rc_core::keymap::Shortcut::parse("alt+pgup").unwrap()),
            "Alt+PgUp"
        );
    }

    #[test]
    fn round_trip_through_shortcut_parsing() {
        for key in [egui::Key::F4, egui::Key::Delete, egui::Key::Home] {
            let sc = shortcut_from_egui(key, egui::Modifiers::NONE).unwrap();
            let text = sc.to_string();
            assert_eq!(rc_core::keymap::Shortcut::parse(&text).unwrap(), sc);
        }
    }
}
