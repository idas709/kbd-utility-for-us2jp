use std::fmt;

pub type ScanCode = u16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDirection {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyFlags {
    pub is_extended: bool,   // E0 prefix
    pub is_extended1: bool,  // E1 prefix
}

impl KeyFlags {
    pub const NORMAL: Self = Self {
        is_extended: false,
        is_extended1: false,
    };
    pub const EXTENDED: Self = Self {
        is_extended: true,
        is_extended1: false,
    };
}

/// A hardware-level or filter-level keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub scan_code: ScanCode,
    pub direction: KeyDirection,
    pub flags: KeyFlags,
    pub time_ms: u32,
}

impl KeyEvent {
    pub fn new(scan_code: ScanCode, direction: KeyDirection, flags: KeyFlags, time_ms: u32) -> Self {
        Self {
            scan_code,
            direction,
            flags,
            time_ms,
        }
    }

    pub fn down(scan_code: ScanCode, flags: KeyFlags) -> Self {
        Self::new(scan_code, KeyDirection::Down, flags, 0)
    }

    pub fn up(scan_code: ScanCode, flags: KeyFlags) -> Self {
        Self::new(scan_code, KeyDirection::Up, flags, 0)
    }

    pub fn is_down(&self) -> bool {
        self.direction == KeyDirection::Down
    }

    pub fn is_up(&self) -> bool {
        self.direction == KeyDirection::Up
    }
}

impl fmt::Display for KeyEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KeyEvent(code=0x{:02X}, dir={:?}, ext={})",
            self.scan_code, self.direction, self.flags.is_extended
        )
    }
}

// Well-known Set 1 scan codes
pub mod scancodes {
    use super::ScanCode;

    pub const ESCAPE: ScanCode = 0x01;
    pub const KEY_1: ScanCode = 0x02;
    pub const KEY_2: ScanCode = 0x03;
    pub const KEY_3: ScanCode = 0x04;
    pub const KEY_4: ScanCode = 0x05;
    pub const KEY_5: ScanCode = 0x06;
    pub const KEY_6: ScanCode = 0x07;
    pub const KEY_7: ScanCode = 0x08;
    pub const KEY_8: ScanCode = 0x09;
    pub const KEY_9: ScanCode = 0x0A;
    pub const KEY_0: ScanCode = 0x0B;
    pub const MINUS: ScanCode = 0x0C;        // US '-', JIS '-'
    pub const EQUALS: ScanCode = 0x0D;       // US '=', JIS '^'
    pub const BACKSPACE: ScanCode = 0x0E;
    pub const TAB: ScanCode = 0x0F;

    pub const KEY_Q: ScanCode = 0x10;
    pub const KEY_W: ScanCode = 0x11;
    pub const KEY_E: ScanCode = 0x12;
    pub const KEY_R: ScanCode = 0x13;
    pub const KEY_T: ScanCode = 0x14;
    pub const KEY_Y: ScanCode = 0x15;
    pub const KEY_U: ScanCode = 0x16;
    pub const KEY_I: ScanCode = 0x17;
    pub const KEY_O: ScanCode = 0x18;
    pub const KEY_P: ScanCode = 0x19;
    pub const LBRACKET: ScanCode = 0x1A;     // US '[', JIS '@'
    pub const RBRACKET: ScanCode = 0x1B;     // US ']', JIS '['
    pub const ENTER: ScanCode = 0x1C;
    pub const LCTRL: ScanCode = 0x1D;

    pub const KEY_A: ScanCode = 0x1E;
    pub const KEY_S: ScanCode = 0x1F;
    pub const KEY_D: ScanCode = 0x20;
    pub const KEY_F: ScanCode = 0x21;
    pub const KEY_G: ScanCode = 0x22;
    pub const KEY_H: ScanCode = 0x23;
    pub const KEY_J: ScanCode = 0x24;
    pub const KEY_K: ScanCode = 0x25;
    pub const KEY_L: ScanCode = 0x26;
    pub const SEMICOLON: ScanCode = 0x27;    // US ';', JIS ';'
    pub const APOSTROPHE: ScanCode = 0x28;   // US ''', JIS ':'
    pub const GRAVE: ScanCode = 0x29;        // US '`', JIS Hankaku/Zenkaku (0x29)
    pub const LSHIFT: ScanCode = 0x2A;
    pub const BACKSLASH: ScanCode = 0x2B;    // US '\', JIS ']'

    pub const KEY_Z: ScanCode = 0x2C;
    pub const KEY_X: ScanCode = 0x2D;
    pub const KEY_C: ScanCode = 0x2E;
    pub const KEY_V: ScanCode = 0x2F;
    pub const KEY_B: ScanCode = 0x30;
    pub const KEY_N: ScanCode = 0x31;
    pub const KEY_M: ScanCode = 0x32;
    pub const COMMA: ScanCode = 0x33;        // ','
    pub const PERIOD: ScanCode = 0x34;       // '.'
    pub const SLASH: ScanCode = 0x35;        // '/'
    pub const RSHIFT: ScanCode = 0x36;
    pub const LALT: ScanCode = 0x38;
    pub const SPACE: ScanCode = 0x39;
    pub const CAPSLOCK: ScanCode = 0x3A;

    // JIS specific scan codes
    pub const JIS_RO: ScanCode = 0x73;       // ろ ('\' / '_')
    pub const JIS_YEN: ScanCode = 0x7D;      // ¥ ('|')
    pub const JIS_MUHENKAN: ScanCode = 0x7B; // 無変換
    pub const JIS_HENKAN: ScanCode = 0x79;   // 変換
    pub const JIS_KANA: ScanCode = 0x70;     // カタカナ/ひらがな

    // Extended keys (E0 prefix)
    pub const RCTRL: ScanCode = 0x1D;        // with E0
    pub const RALT: ScanCode = 0x38;         // with E0 (AltGr)
    pub const LWIN: ScanCode = 0x5B;         // with E0
    pub const RWIN: ScanCode = 0x5C;         // with E0
    pub const APPS: ScanCode = 0x5D;         // with E0
}

/// Tracks the active state of modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierState {
    pub lshift: bool,
    pub rshift: bool,
    pub lctrl: bool,
    pub rctrl: bool,
    pub lalt: bool,
    pub ralt: bool,
    pub lwin: bool,
    pub rwin: bool,
}

impl ModifierState {
    pub fn is_shift(&self) -> bool {
        self.lshift || self.rshift
    }

    pub fn is_ctrl(&self) -> bool {
        self.lctrl || self.rctrl
    }

    pub fn is_alt(&self) -> bool {
        self.lalt || self.ralt
    }

    pub fn is_win(&self) -> bool {
        self.lwin || self.rwin
    }

    pub fn has_shortcut_modifier(&self) -> bool {
        self.is_ctrl() || self.is_alt() || self.is_win()
    }

    pub fn update(&mut self, event: &KeyEvent) {
        let is_down = event.is_down();
        let ext = event.flags.is_extended;

        match (event.scan_code, ext) {
            (scancodes::LSHIFT, false) => self.lshift = is_down,
            (scancodes::RSHIFT, false) => self.rshift = is_down,
            (scancodes::LCTRL, false) => self.lctrl = is_down,
            (scancodes::RCTRL, true) => self.rctrl = is_down,
            (scancodes::LALT, false) => self.lalt = is_down,
            (scancodes::RALT, true) => self.ralt = is_down,
            (scancodes::LWIN, true) => self.lwin = is_down,
            (scancodes::RWIN, true) => self.rwin = is_down,
            _ => {}
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

