use hid_remap_core::config::LayoutMode;
use hid_remap_core::engine::{EngineAction, RemapEngine};
use hid_remap_core::ime::{CtrlTapTracker, TapResult};
use hid_remap_core::keyboard::scancodes::*;
use hid_remap_core::keyboard::{KeyEvent, KeyFlags};

// Helper to extract emitted events
fn emit_events(action: EngineAction) -> Vec<KeyEvent> {
    match action {
        EngineAction::Emit(evts) => evts,
        EngineAction::Suppress => Vec::new(),
    }
}

/// AT-05: US->JIS alphanumeric keys pass through directly
#[test]
fn test_at_05_us_to_jis_alphanumeric() {
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    // Letter 'A'
    let down_a = KeyEvent::down(KEY_A, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(down_a)), vec![down_a]);
    let up_a = KeyEvent::up(KEY_A, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(up_a)), vec![up_a]);

    // Number '1'
    let down_1 = KeyEvent::down(KEY_1, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(down_1)), vec![down_1]);
    let up_1 = KeyEvent::up(KEY_1, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(up_1)), vec![up_1]);
}

/// AT-06: US->JIS symbols map to correspond to US keycap legends
#[test]
fn test_at_06_us_to_jis_symbols_all() {
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    // 1. '@' = Shift + 2 -> JIS '@' (0x1A) unshifted
    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    let out = emit_events(engine.process(KeyEvent::down(KEY_2, KeyFlags::NORMAL)));
    assert_eq!(out, vec![
        KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
        KeyEvent::down(LBRACKET, KeyFlags::NORMAL), // JIS '@'
    ]);
    let out_up = emit_events(engine.process(KeyEvent::up(KEY_2, KeyFlags::NORMAL)));
    assert_eq!(out_up, vec![
        KeyEvent::up(LBRACKET, KeyFlags::NORMAL),
        KeyEvent::down(LSHIFT, KeyFlags::NORMAL),
    ]);
    engine.process(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));

    // 2. '^' = Shift + 6 -> JIS '^' (0x0D) unshifted
    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    let out = emit_events(engine.process(KeyEvent::down(KEY_6, KeyFlags::NORMAL)));
    assert_eq!(out, vec![
        KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
        KeyEvent::down(EQUALS, KeyFlags::NORMAL), // JIS '^'
    ]);
    engine.process(KeyEvent::up(KEY_6, KeyFlags::NORMAL));
    engine.process(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));

    // 3. '=' = US '=' unshifted -> JIS Shift + '-' (0x0C)
    let out = emit_events(engine.process(KeyEvent::down(EQUALS, KeyFlags::NORMAL)));
    assert_eq!(out, vec![
        KeyEvent::down(LSHIFT, KeyFlags::NORMAL),
        KeyEvent::down(MINUS, KeyFlags::NORMAL),
    ]);
    let out_up = emit_events(engine.process(KeyEvent::up(EQUALS, KeyFlags::NORMAL)));
    assert_eq!(out_up, vec![
        KeyEvent::up(MINUS, KeyFlags::NORMAL),
        KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
    ]);

    // 4. '+' = US Shift + '=' -> JIS Shift + ';' (0x27)
    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    let out = emit_events(engine.process(KeyEvent::down(EQUALS, KeyFlags::NORMAL)));
    assert_eq!(out, vec![KeyEvent::down(SEMICOLON, KeyFlags::NORMAL)]);
    engine.process(KeyEvent::up(EQUALS, KeyFlags::NORMAL));
    engine.process(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));

    // 5. ':' = US Shift + ';' -> JIS ':' (0x28) unshifted
    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    let out = emit_events(engine.process(KeyEvent::down(SEMICOLON, KeyFlags::NORMAL)));
    assert_eq!(out, vec![
        KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
        KeyEvent::down(APOSTROPHE, KeyFlags::NORMAL), // JIS ':'
    ]);
    engine.process(KeyEvent::up(SEMICOLON, KeyFlags::NORMAL));
    engine.process(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));
}

/// AT-07 & AT-08: JIS->US mapping
#[test]
fn test_at_07_08_jis_to_us() {
    let mut engine = RemapEngine::new(LayoutMode::JisToUs);

    // JIS '@' (0x1A) key pressed on JIS hardware -> behaves as US '[' (0x1B on JIS)
    let out = emit_events(engine.process(KeyEvent::down(LBRACKET, KeyFlags::NORMAL)));
    assert_eq!(out, vec![KeyEvent::down(RBRACKET, KeyFlags::NORMAL)]);
    let out_up = emit_events(engine.process(KeyEvent::up(LBRACKET, KeyFlags::NORMAL)));
    assert_eq!(out_up, vec![KeyEvent::up(RBRACKET, KeyFlags::NORMAL)]);
}

/// AT-09: Shift combinations work seamlessly
#[test]
fn test_at_09_shift_combination() {
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    // Shift + A -> Capital A (Pass through unchanged)
    let s_down = KeyEvent::down(LSHIFT, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(s_down)), vec![s_down]);

    let a_down = KeyEvent::down(KEY_A, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(a_down)), vec![a_down]);

    let a_up = KeyEvent::up(KEY_A, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(a_up)), vec![a_up]);

    let s_up = KeyEvent::up(LSHIFT, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(s_up)), vec![s_up]);
}

/// AT-10: Ctrl shortcuts maintain normal behavior
#[test]
fn test_at_10_ctrl_shortcuts() {
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    // Ctrl + [
    engine.process(KeyEvent::down(LCTRL, KeyFlags::NORMAL));
    let out = emit_events(engine.process(KeyEvent::down(LBRACKET, KeyFlags::NORMAL)));
    // Should NOT be remapped to JIS 0x1B, but pass through as original LBRACKET (0x1A)
    assert_eq!(out, vec![KeyEvent::down(LBRACKET, KeyFlags::NORMAL)]);

    engine.process(KeyEvent::up(LBRACKET, KeyFlags::NORMAL));
    engine.process(KeyEvent::up(LCTRL, KeyFlags::NORMAL));
}

/// AT-11: Passthrough mode leaves input completely unchanged
#[test]
fn test_at_11_passthrough_mode() {
    let mut engine = RemapEngine::new(LayoutMode::Passthrough);

    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    let out = emit_events(engine.process(KeyEvent::down(KEY_2, KeyFlags::NORMAL)));
    assert_eq!(out, vec![KeyEvent::down(KEY_2, KeyFlags::NORMAL)]);
}

/// AT-12: Left Ctrl single tap triggers IME OFF
#[test]
fn test_at_12_left_ctrl_tap() {
    let mut tracker = CtrlTapTracker::new(true, 1000);
    assert_eq!(
        tracker.process_event(&KeyEvent::down(LCTRL, KeyFlags::NORMAL)),
        TapResult::None
    );
    assert_eq!(
        tracker.process_event(&KeyEvent::up(LCTRL, KeyFlags::NORMAL)),
        TapResult::LeftCtrlTap
    );
}

/// AT-13: Right Ctrl single tap triggers IME ON
#[test]
fn test_at_13_right_ctrl_tap() {
    let mut tracker = CtrlTapTracker::new(true, 1000);
    assert_eq!(
        tracker.process_event(&KeyEvent::down(RCTRL, KeyFlags::EXTENDED)),
        TapResult::None
    );
    assert_eq!(
        tracker.process_event(&KeyEvent::up(RCTRL, KeyFlags::EXTENDED)),
        TapResult::RightCtrlTap
    );
}

/// AT-14 & AT-15: Ctrl+C and Ctrl+V do not trigger IME switch
#[test]
fn test_at_14_15_ctrl_shortcuts_ignore_ime() {
    let mut tracker = CtrlTapTracker::new(true, 1000);

    // Ctrl + C
    tracker.process_event(&KeyEvent::down(LCTRL, KeyFlags::NORMAL));
    tracker.process_event(&KeyEvent::down(KEY_C, KeyFlags::NORMAL));
    tracker.process_event(&KeyEvent::up(KEY_C, KeyFlags::NORMAL));
    let res = tracker.process_event(&KeyEvent::up(LCTRL, KeyFlags::NORMAL));
    assert_eq!(res, TapResult::None);

    // Ctrl + V
    tracker.process_event(&KeyEvent::down(LCTRL, KeyFlags::NORMAL));
    tracker.process_event(&KeyEvent::down(KEY_V, KeyFlags::NORMAL));
    tracker.process_event(&KeyEvent::up(KEY_V, KeyFlags::NORMAL));
    let res = tracker.process_event(&KeyEvent::up(LCTRL, KeyFlags::NORMAL));
    assert_eq!(res, TapResult::None);
}

/// AT-16: Ctrl held beyond timeout does not trigger IME switch
#[test]
fn test_at_16_ctrl_long_press_timeout() {
    let mut tracker = CtrlTapTracker::new(true, 500);
    tracker.process_event(&KeyEvent::new(LCTRL, hid_remap_core::keyboard::KeyDirection::Down, KeyFlags::NORMAL, 1000));
    let res = tracker.process_event(&KeyEvent::new(LCTRL, hid_remap_core::keyboard::KeyDirection::Up, KeyFlags::NORMAL, 1600));
    assert_eq!(res, TapResult::None);
}

/// AT-17: Device with Ctrl IME disabled operates normally
#[test]
fn test_at_17_ctrl_ime_disabled_device() {
    let mut tracker = CtrlTapTracker::new(false, 1000);
    tracker.process_event(&KeyEvent::down(LCTRL, KeyFlags::NORMAL));
    let res = tracker.process_event(&KeyEvent::up(LCTRL, KeyFlags::NORMAL));
    assert_eq!(res, TapResult::None);
}

/// AT-18: Layout conversion OFF + IME switch ON operates independently
#[test]
fn test_at_18_layout_off_ime_on() {
    let mut engine = RemapEngine::new(LayoutMode::Passthrough);
    let mut tracker = CtrlTapTracker::new(true, 1000);

    // Layout conversion does nothing
    let evt = KeyEvent::down(KEY_A, KeyFlags::NORMAL);
    assert_eq!(emit_events(engine.process(evt)), vec![evt]);

    // IME tap still works
    tracker.process_event(&KeyEvent::down(LCTRL, KeyFlags::NORMAL));
    assert_eq!(
        tracker.process_event(&KeyEvent::up(LCTRL, KeyFlags::NORMAL)),
        TapResult::LeftCtrlTap
    );
}

/// AT-20: State integrity during rapid / overlapping events
#[test]
fn test_at_20_overlapping_keys_integrity() {
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    // Press Shift
    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    // Press '2' (inverts Shift to release it, presses 0x1A)
    engine.process(KeyEvent::down(KEY_2, KeyFlags::NORMAL));
    // Release Shift while '2' is still held!
    engine.process(KeyEvent::up(LSHIFT, KeyFlags::NORMAL));
    // Release '2'
    let out = emit_events(engine.process(KeyEvent::up(KEY_2, KeyFlags::NORMAL)));
    // Should safely release 0x1A, and because physical Shift was released, should NOT re-press Shift!
    assert_eq!(out, vec![KeyEvent::up(LBRACKET, KeyFlags::NORMAL)]);
    assert!(!engine.modifiers.is_shift());
}

/// AT-21: Device disconnection while keys held does not leave stuck keys
#[test]
fn test_at_21_disconnection_clean_reset() {
    let mut engine = RemapEngine::new(LayoutMode::UsToJis);

    engine.process(KeyEvent::down(LSHIFT, KeyFlags::NORMAL));
    engine.process(KeyEvent::down(KEY_A, KeyFlags::NORMAL));

    let cleanups = engine.reset();
    assert_eq!(cleanups, vec![
        KeyEvent::up(LSHIFT, KeyFlags::NORMAL),
    ]);
    assert!(!engine.modifiers.is_shift());
}
