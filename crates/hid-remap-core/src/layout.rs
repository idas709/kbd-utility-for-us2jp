use crate::keyboard::scancodes::*;
use crate::keyboard::{KeyFlags, ScanCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetShiftState {
    Preserve,
    RequireShift,
    RequireNoShift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyMapping {
    pub target_scan_code: ScanCode,
    pub target_flags: KeyFlags,
    pub target_shift: TargetShiftState,
}

impl KeyMapping {
    pub const fn new(
        target_scan_code: ScanCode,
        target_flags: KeyFlags,
        target_shift: TargetShiftState,
    ) -> Self {
        Self {
            target_scan_code,
            target_flags,
            target_shift,
        }
    }
}

/// Resolves mapping for US physical keyboard connected to a Windows JIS layout environment.
pub fn map_us_physical_to_jis(scan_code: ScanCode, physical_shift: bool) -> Option<KeyMapping> {
    match (scan_code, physical_shift) {
        // Number row symbols
        (KEY_2, true) => {
            // US: Shift + 2 = '@' -> JIS: '@' (0x1A, unshifted)
            Some(KeyMapping::new(
                LBRACKET, // In JIS layout, 0x1A is '@'
                KeyFlags::NORMAL,
                TargetShiftState::RequireNoShift,
            ))
        }
        (KEY_6, true) => {
            // US: Shift + 6 = '^' -> JIS: '^' (0x0D, unshifted)
            Some(KeyMapping::new(
                EQUALS, // In JIS layout, 0x0D is '^'
                KeyFlags::NORMAL,
                TargetShiftState::RequireNoShift,
            ))
        }
        (KEY_7, true) => {
            // US: Shift + 7 = '&' -> JIS: Shift + 6 (0x07)
            Some(KeyMapping::new(
                KEY_6,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (KEY_8, true) => {
            // US: Shift + 8 = '*' -> JIS: Shift + : (0x28)
            Some(KeyMapping::new(
                APOSTROPHE, // In JIS layout, 0x28 is ':'
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (KEY_9, true) => {
            // US: Shift + 9 = '(' -> JIS: Shift + 8 (0x09)
            Some(KeyMapping::new(
                KEY_8,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (KEY_0, true) => {
            // US: Shift + 0 = ')' -> JIS: Shift + 9 (0x0A)
            Some(KeyMapping::new(
                KEY_9,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (MINUS, true) => {
            // US: Shift + - = '_' -> JIS: Shift + ろ (0x73)
            Some(KeyMapping::new(
                JIS_RO,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (EQUALS, false) => {
            // US: '=' -> JIS: Shift + - (0x0C)
            Some(KeyMapping::new(
                MINUS,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (EQUALS, true) => {
            // US: Shift + = = '+' -> JIS: Shift + ; (0x27)
            Some(KeyMapping::new(
                SEMICOLON,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }

        // Grave / Tilde
        (GRAVE, false) => {
            // US: '`' -> JIS: Shift + @ (0x1A)
            Some(KeyMapping::new(
                LBRACKET, // JIS '@'
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (GRAVE, true) => {
            // US: '~' -> JIS: Shift + ^ (0x0D)
            Some(KeyMapping::new(
                EQUALS, // JIS '^'
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }

        // Brackets
        (LBRACKET, false) => {
            // US: '[' -> JIS: '[' (0x1B, unshifted)
            Some(KeyMapping::new(
                RBRACKET, // In JIS, 0x1B is '['
                KeyFlags::NORMAL,
                TargetShiftState::RequireNoShift,
            ))
        }
        (LBRACKET, true) => {
            // US: '{' -> JIS: Shift + [ (0x1B)
            Some(KeyMapping::new(
                RBRACKET,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (RBRACKET, false) => {
            // US: ']' -> JIS: ']' (0x2B, unshifted)
            Some(KeyMapping::new(
                BACKSLASH, // In JIS, 0x2B is ']'
                KeyFlags::NORMAL,
                TargetShiftState::RequireNoShift,
            ))
        }
        (RBRACKET, true) => {
            // US: '}' -> JIS: Shift + ] (0x2B)
            Some(KeyMapping::new(
                BACKSLASH,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }

        // Backslash / Pipe (Key above Enter on US)
        (BACKSLASH, false) => {
            // US: '\' -> JIS: '¥' (0x7D, unshifted)
            Some(KeyMapping::new(
                JIS_YEN,
                KeyFlags::NORMAL,
                TargetShiftState::RequireNoShift,
            ))
        }
        (BACKSLASH, true) => {
            // US: '|' -> JIS: Shift + ¥ (0x7D)
            Some(KeyMapping::new(
                JIS_YEN,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }

        // Semicolon / Colon
        (SEMICOLON, true) => {
            // US: Shift + ; = ':' -> JIS: ':' (0x28, unshifted)
            Some(KeyMapping::new(
                APOSTROPHE, // In JIS, 0x28 is ':'
                KeyFlags::NORMAL,
                TargetShiftState::RequireNoShift,
            ))
        }

        // Apostrophe / Quote
        (APOSTROPHE, false) => {
            // US: ''' -> JIS: Shift + 7 (0x08)
            Some(KeyMapping::new(
                KEY_7,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }
        (APOSTROPHE, true) => {
            // US: '"' -> JIS: Shift + 2 (0x03)
            Some(KeyMapping::new(
                KEY_2,
                KeyFlags::NORMAL,
                TargetShiftState::RequireShift,
            ))
        }

        // CapsLock (0x3A)
        // On US keyboards, CapsLock toggles CapsLock directly.
        // On JIS Windows environment, Windows requires Shift + CapsLock to toggle CapsLock.
        // Mapping CAPSLOCK to RequireShift ensures pressing CapsLock toggles and releases CapsLock cleanly!
        (CAPSLOCK, false) => Some(KeyMapping::new(
            CAPSLOCK,
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),
        (CAPSLOCK, true) => Some(KeyMapping::new(
            CAPSLOCK,
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),

        // All other keys pass through unchanged
        _ => None,
    }
}

/// Resolves mapping for JIS physical keyboard emulating US layout on JIS OS environment.
pub fn map_jis_physical_to_us(scan_code: ScanCode, physical_shift: bool) -> Option<KeyMapping> {
    match (scan_code, physical_shift) {
        // JIS '@' (0x1A) key -> behaves as US '['
        (LBRACKET, false) => Some(KeyMapping::new(
            RBRACKET, // Output '[' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireNoShift,
        )),
        (LBRACKET, true) => Some(KeyMapping::new(
            RBRACKET, // Output '{' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),

        // JIS '[' (0x1B) key -> behaves as US ']'
        (RBRACKET, false) => Some(KeyMapping::new(
            BACKSLASH, // Output ']' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireNoShift,
        )),
        (RBRACKET, true) => Some(KeyMapping::new(
            BACKSLASH, // Output '}' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),

        // JIS ':' (0x28) key -> behaves as US ''' / '"'
        (APOSTROPHE, false) => Some(KeyMapping::new(
            KEY_7, // Output ''' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),
        (APOSTROPHE, true) => Some(KeyMapping::new(
            KEY_2, // Output '"' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),

        // JIS '^' (0x0D) key -> behaves as US '=' / '+'
        (EQUALS, false) => Some(KeyMapping::new(
            MINUS, // Output '=' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),
        (EQUALS, true) => Some(KeyMapping::new(
            SEMICOLON, // Output '+' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),

        // JIS ']' (0x2B) key -> behaves as US '\' / '|'
        (BACKSLASH, false) => Some(KeyMapping::new(
            JIS_YEN, // Output '\' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireNoShift,
        )),
        (BACKSLASH, true) => Some(KeyMapping::new(
            JIS_YEN, // Output '|' in JIS
            KeyFlags::NORMAL,
            TargetShiftState::RequireShift,
        )),

        _ => None,
    }
}

