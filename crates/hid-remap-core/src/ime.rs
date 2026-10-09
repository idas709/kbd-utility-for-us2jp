use crate::keyboard::scancodes::{LCTRL, RCTRL};
use crate::keyboard::{KeyEvent, ScanCode};

pub const DEFAULT_TIMEOUT_MS: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapResult {
    None,
    LeftCtrlTap,
    RightCtrlTap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingCtrl {
    None,
    Left { pressed_time: u32 },
    Right { pressed_time: u32 },
}

/// Tracks Ctrl single-tap events per keyboard device.
#[derive(Debug, Clone)]
pub struct CtrlTapTracker {
    pub enabled: bool,
    pub timeout_ms: u32,
    pending: PendingCtrl,
}

impl CtrlTapTracker {
    pub fn new(enabled: bool, timeout_ms: u32) -> Self {
        Self {
            enabled,
            timeout_ms,
            pending: PendingCtrl::None,
        }
    }

    pub fn reset(&mut self) {
        self.pending = PendingCtrl::None;
    }

    pub fn process_event(&mut self, event: &KeyEvent) -> TapResult {
        if !self.enabled {
            self.reset();
            return TapResult::None;
        }

        if event.is_down() {
            self.on_key_down(event.scan_code, event.flags.is_extended, event.time_ms);
            TapResult::None
        } else {
            self.on_key_up(event.scan_code, event.flags.is_extended, event.time_ms)
        }
    }

    pub fn on_key_down(&mut self, scan_code: ScanCode, is_extended: bool, time_ms: u32) {
        let is_left = scan_code == LCTRL && !is_extended;
        let is_right = scan_code == RCTRL && is_extended;

        if is_left {
            match self.pending {
                PendingCtrl::Left { .. } => {
                    // Key repeat: keep initial pressed_time
                }
                _ => {
                    self.pending = PendingCtrl::Left {
                        pressed_time: time_ms,
                    };
                }
            }
        } else if is_right {
            match self.pending {
                PendingCtrl::Right { .. } => {
                    // Key repeat: keep initial pressed_time
                }
                _ => {
                    self.pending = PendingCtrl::Right {
                        pressed_time: time_ms,
                    };
                }
            }
        } else {
            // Any other key cancels the single-tap gesture (e.g. Ctrl + C, Ctrl + V)
            self.pending = PendingCtrl::None;
        }
    }

    pub fn on_key_up(&mut self, scan_code: ScanCode, is_extended: bool, time_ms: u32) -> TapResult {
        let is_left = scan_code == LCTRL && !is_extended;
        let is_right = scan_code == RCTRL && is_extended;

        let mut result = TapResult::None;

        if is_left {
            if let PendingCtrl::Left { pressed_time } = self.pending {
                let elapsed = time_ms.wrapping_sub(pressed_time);
                if elapsed <= self.timeout_ms {
                    result = TapResult::LeftCtrlTap;
                }
            }
            self.pending = PendingCtrl::None;
        } else if is_right {
            if let PendingCtrl::Right { pressed_time } = self.pending {
                let elapsed = time_ms.wrapping_sub(pressed_time);
                if elapsed <= self.timeout_ms {
                    result = TapResult::RightCtrlTap;
                }
            }
            self.pending = PendingCtrl::None;
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::scancodes::KEY_C;
    use crate::keyboard::KeyFlags;

    #[test]
    fn test_left_ctrl_tap_success() {
        let mut tracker = CtrlTapTracker::new(true, 1000);
        let res1 = tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Down, KeyFlags::NORMAL, 100));
        assert_eq!(res1, TapResult::None);

        let res2 = tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Up, KeyFlags::NORMAL, 250));
        assert_eq!(res2, TapResult::LeftCtrlTap);
    }

    #[test]
    fn test_right_ctrl_tap_success() {
        let mut tracker = CtrlTapTracker::new(true, 1000);
        tracker.process_event(&KeyEvent::new(RCTRL, crate::keyboard::KeyDirection::Down, KeyFlags::EXTENDED, 100));
        let res = tracker.process_event(&KeyEvent::new(RCTRL, crate::keyboard::KeyDirection::Up, KeyFlags::EXTENDED, 200));
        assert_eq!(res, TapResult::RightCtrlTap);
    }

    #[test]
    fn test_ctrl_shortcut_cancels_tap() {
        let mut tracker = CtrlTapTracker::new(true, 1000);
        tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Down, KeyFlags::NORMAL, 100));
        tracker.process_event(&KeyEvent::new(KEY_C, crate::keyboard::KeyDirection::Down, KeyFlags::NORMAL, 150));
        tracker.process_event(&KeyEvent::new(KEY_C, crate::keyboard::KeyDirection::Up, KeyFlags::NORMAL, 200));
        let res = tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Up, KeyFlags::NORMAL, 250));
        assert_eq!(res, TapResult::None);
    }

    #[test]
    fn test_long_press_exceeds_timeout() {
        let mut tracker = CtrlTapTracker::new(true, 500);
        tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Down, KeyFlags::NORMAL, 1000));
        let res = tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Up, KeyFlags::NORMAL, 1600));
        assert_eq!(res, TapResult::None);
    }

    #[test]
    fn test_disabled_tracker_does_nothing() {
        let mut tracker = CtrlTapTracker::new(false, 1000);
        tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Down, KeyFlags::NORMAL, 100));
        let res = tracker.process_event(&KeyEvent::new(LCTRL, crate::keyboard::KeyDirection::Up, KeyFlags::NORMAL, 200));
        assert_eq!(res, TapResult::None);
    }
}

