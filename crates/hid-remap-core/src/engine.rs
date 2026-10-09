use crate::config::LayoutMode;
use crate::keyboard::scancodes::*;
use crate::keyboard::{KeyDirection, KeyEvent, KeyFlags, ModifierState, ScanCode};
use crate::layout::{
    map_jis_physical_to_us, map_us_physical_to_jis, TargetShiftState,
};
use std::collections::HashMap;

/// An action produced by the remap engine for an incoming key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineAction {
    /// Emit a sequence of transformed key events.
    Emit(Vec<KeyEvent>),
    /// Suppress the input event completely.
    Suppress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveMapping {
    target_scan_code: ScanCode,
    target_flags: KeyFlags,
    injected_shift_down: bool,
    injected_shift_up: bool,
}

/// Stateful keyboard remapping engine for a single keyboard device.
#[derive(Debug)]
pub struct RemapEngine {
    pub layout_mode: LayoutMode,
    pub modifiers: ModifierState,
    active_keys: HashMap<ScanCode, ActiveMapping>,
}

impl RemapEngine {
    pub fn new(layout_mode: LayoutMode) -> Self {
        Self {
            layout_mode,
            modifiers: ModifierState::default(),
            active_keys: HashMap::new(),
        }
    }

    /// Reset internal state and return any necessary key up events to restore clean keyboard state.
    pub fn reset(&mut self) -> Vec<KeyEvent> {
        let mut cleanups = Vec::new();

        // Release all active mapped keys
        for (&scan_code, active) in &self.active_keys {
            cleanups.push(KeyEvent::up(active.target_scan_code, active.target_flags));
            if active.injected_shift_down {
                cleanups.push(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));
            }
            if active.injected_shift_up {
                cleanups.push(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
            }
            let _ = scan_code;
        }
        self.active_keys.clear();

        // If physical modifiers were recorded as pressed, release them
        if self.modifiers.lshift {
            cleanups.push(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));
        }
        if self.modifiers.rshift {
            cleanups.push(KeyEvent::up(RSHIFT, KeyFlags::NORMAL));
        }
        if self.modifiers.lctrl {
            cleanups.push(KeyEvent::up(LCTRL, KeyFlags::NORMAL));
        }
        if self.modifiers.rctrl {
            cleanups.push(KeyEvent::up(RCTRL, KeyFlags::EXTENDED));
        }
        if self.modifiers.lalt {
            cleanups.push(KeyEvent::up(LALT, KeyFlags::NORMAL));
        }
        if self.modifiers.ralt {
            cleanups.push(KeyEvent::up(RALT, KeyFlags::EXTENDED));
        }
        if self.modifiers.lwin {
            cleanups.push(KeyEvent::up(LWIN, KeyFlags::EXTENDED));
        }
        if self.modifiers.rwin {
            cleanups.push(KeyEvent::up(RWIN, KeyFlags::EXTENDED));
        }

        self.modifiers.reset();
        cleanups
    }

    /// Process an incoming hardware key event and return the transformed sequence.
    pub fn process(&mut self, event: KeyEvent) -> EngineAction {
        // Track modifier keys
        self.modifiers.update(&event);

        // If passthrough or modifier itself, pass through directly
        if self.layout_mode == LayoutMode::Passthrough || is_modifier_key(event.scan_code, event.flags) {
            return EngineAction::Emit(vec![event]);
        }

        // FR-015: Maintain standard shortcuts when Ctrl, Alt, or Win are held
        if self.modifiers.has_shortcut_modifier() {
            // If the key was already active in a remapped state, release it using the remapped form
            if event.is_up() {
                if let Some(active) = self.active_keys.remove(&event.scan_code) {
                    return EngineAction::Emit(self.generate_release_events(&active));
                }
            }
            return EngineAction::Emit(vec![event]);
        }

        match event.direction {
            KeyDirection::Down => self.handle_key_down(event),
            KeyDirection::Up => self.handle_key_up(event),
        }
    }

    fn handle_key_down(&mut self, event: KeyEvent) -> EngineAction {
        // Check for key repeat: if already active, re-emit target key down without re-injecting Shift
        if let Some(active) = self.active_keys.get(&event.scan_code) {
            return EngineAction::Emit(vec![KeyEvent::new(
                active.target_scan_code,
                KeyDirection::Down,
                active.target_flags,
                event.time_ms,
            )]);
        }

        let physical_shift = self.modifiers.is_shift();
        let mapping = match self.layout_mode {
            LayoutMode::UsToJis => map_us_physical_to_jis(event.scan_code, physical_shift),
            LayoutMode::JisToUs => map_jis_physical_to_us(event.scan_code, physical_shift),
            LayoutMode::Passthrough => None,
        };

        if let Some(map) = mapping {
            let mut events = Vec::new();
            let mut injected_shift_down = false;
            let mut injected_shift_up = false;

            match map.target_shift {
                TargetShiftState::RequireShift => {
                    if !physical_shift {
                        // Invert: inject Shift Down
                        events.push(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
                        injected_shift_down = true;
                    }
                }
                TargetShiftState::RequireNoShift => {
                    if physical_shift {
                        // Invert: inject Shift Up
                        events.push(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));
                        injected_shift_up = true;
                    }
                }
                TargetShiftState::Preserve => {}
            }

            // Emit the transformed target key
            events.push(KeyEvent::new(
                map.target_scan_code,
                KeyDirection::Down,
                map.target_flags,
                event.time_ms,
            ));

            self.active_keys.insert(
                event.scan_code,
                ActiveMapping {
                    target_scan_code: map.target_scan_code,
                    target_flags: map.target_flags,
                    injected_shift_down,
                    injected_shift_up,
                },
            );

            EngineAction::Emit(events)
        } else {
            // No mapping needed -> pass through
            EngineAction::Emit(vec![event])
        }
    }

    fn handle_key_up(&mut self, event: KeyEvent) -> EngineAction {
        if let Some(active) = self.active_keys.remove(&event.scan_code) {
            EngineAction::Emit(self.generate_release_events(&active))
        } else {
            // Unmapped key release -> pass through
            EngineAction::Emit(vec![event])
        }
    }

    fn generate_release_events(&self, active: &ActiveMapping) -> Vec<KeyEvent> {
        let mut events = Vec::new();

        // 1. Release target key
        events.push(KeyEvent::up(active.target_scan_code, active.target_flags));

        // 2. Restore shift state if temporarily inverted
        if active.injected_shift_down {
            // We injected Shift Down before, so release it now
            events.push(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));
        }

        if active.injected_shift_up && self.modifiers.is_shift() {
            // We temporarily released Shift, but physical Shift is still pressed -> re-press Shift
            events.push(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
        }

        events
    }
}

fn is_modifier_key(scan_code: ScanCode, flags: KeyFlags) -> bool {
    match (scan_code, flags.is_extended) {
        (LSHIFT, false)
        | (RSHIFT, false)
        | (LCTRL, false)
        | (RCTRL, true)
        | (LALT, false)
        | (RALT, true)
        | (LWIN, true)
        | (RWIN, true) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passthrough_mode() {
        let mut engine = RemapEngine::new(LayoutMode::Passthrough);
        let event = KeyEvent::down(KEY_A, KeyFlags::NORMAL);
        let action = engine.process(event);
        assert_eq!(action, EngineAction::Emit(vec![event]));
    }

    #[test]
    fn test_us_to_jis_at_sign_shift_2() {
        let mut engine = RemapEngine::new(LayoutMode::UsToJis);

        // 1. Press Shift
        let shift_down = KeyEvent::down(LSHIFT, KeyFlags::NORMAL);
        assert_eq!(engine.process(shift_down), EngineAction::Emit(vec![shift_down]));

        // 2. Press '2' (US '@') -> should temporarily release Shift and press JIS '@' (0x1A)
        let key_2_down = KeyEvent::down(KEY_2, KeyFlags::NORMAL);
        let action = engine.process(key_2_down);
        assert_eq!(
            action,
            EngineAction::Emit(vec![
                KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
                KeyEvent::down(LBRACKET, KeyFlags::NORMAL), // JIS '@'
            ])
        );

        // 3. Release '2' -> release JIS '@' (0x1A) and re-press Shift
        let key_2_up = KeyEvent::up(KEY_2, KeyFlags::NORMAL);
        let action_up = engine.process(key_2_up);
        assert_eq!(
            action_up,
            EngineAction::Emit(vec![
                KeyEvent::up(LBRACKET, KeyFlags::NORMAL),
                KeyEvent::down(LSHIFT, KeyFlags::NORMAL),
            ])
        );

        // 4. Release Shift
        let shift_up = KeyEvent::up(LSHIFT, KeyFlags::NORMAL);
        assert_eq!(engine.process(shift_up), EngineAction::Emit(vec![shift_up]));
    }

    #[test]
    fn test_us_to_jis_single_quote() {
        let mut engine = RemapEngine::new(LayoutMode::UsToJis);

        // Press ''' without Shift -> should inject Shift + 7 (0x08)
        let quote_down = KeyEvent::down(APOSTROPHE, KeyFlags::NORMAL);
        let action = engine.process(quote_down);
        assert_eq!(
            action,
            EngineAction::Emit(vec![
                KeyEvent::down(LSHIFT, KeyFlags::NORMAL),
                KeyEvent::down(KEY_7, KeyFlags::NORMAL),
            ])
        );

        // Release ''' -> release 7 and release Shift
        let quote_up = KeyEvent::up(APOSTROPHE, KeyFlags::NORMAL);
        let action_up = engine.process(quote_up);
        assert_eq!(
            action_up,
            EngineAction::Emit(vec![
                KeyEvent::up(KEY_7, KeyFlags::NORMAL),
                KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
            ])
        );
    }

    #[test]
    fn test_us_to_jis_double_quote_shift_apostrophe() {
        let mut engine = RemapEngine::new(LayoutMode::UsToJis);

        // Press Shift
        let shift_down = KeyEvent::down(LSHIFT, KeyFlags::NORMAL);
        engine.process(shift_down);

        // Press Shift + ''' -> should output Shift + 2 (0x03)
        let apostrophe_down = KeyEvent::down(APOSTROPHE, KeyFlags::NORMAL);
        let action = engine.process(apostrophe_down);
        // Shift is already held, so no extra Shift injected
        assert_eq!(
            action,
            EngineAction::Emit(vec![KeyEvent::down(KEY_2, KeyFlags::NORMAL)])
        );

        // Release '''
        let apostrophe_up = KeyEvent::up(APOSTROPHE, KeyFlags::NORMAL);
        let action_up = engine.process(apostrophe_up);
        assert_eq!(
            action_up,
            EngineAction::Emit(vec![KeyEvent::up(KEY_2, KeyFlags::NORMAL)])
        );
    }

    #[test]
    fn test_shortcut_protection_ctrl_c() {
        let mut engine = RemapEngine::new(LayoutMode::UsToJis);

        // Press Ctrl
        let ctrl_down = KeyEvent::down(LCTRL, KeyFlags::NORMAL);
        engine.process(ctrl_down);

        // Press 'C'
        let c_down = KeyEvent::down(KEY_C, KeyFlags::NORMAL);
        let action = engine.process(c_down);
        assert_eq!(action, EngineAction::Emit(vec![c_down]));

        // Release 'C'
        let c_up = KeyEvent::up(KEY_C, KeyFlags::NORMAL);
        let action_up = engine.process(c_up);
        assert_eq!(action_up, EngineAction::Emit(vec![c_up]));
    }

    #[test]
    fn test_reset_clears_stuck_keys() {
        let mut engine = RemapEngine::new(LayoutMode::UsToJis);

        // Press ''' (injects Shift + 7)
        engine.process(KeyEvent::down(APOSTROPHE, KeyFlags::NORMAL));

        // Device disconnected or reset called
        let cleanups = engine.reset();
        assert_eq!(
            cleanups,
            vec![
                KeyEvent::up(KEY_7, KeyFlags::NORMAL),
                KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
            ]
        );

        // Subsequent key should start completely clean
        assert!(!engine.modifiers.is_shift());
    }
}

