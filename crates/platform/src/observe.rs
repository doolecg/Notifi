//! Watching what is typed and what the pointer does, for typed snippet triggers and the Shelf's
//! drag gestures. Events are only read, never changed or swallowed.

use crate::common::EventStream;
use crate::error::Result;
use crate::traits::MouseButton;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Observed {
    /// A character the key press produced in the current keyboard layout.
    Text(char),
    Backspace,
    /// A key that ends a typed word: Enter, Tab, Escape, an arrow, or a shortcut with Ctrl or Alt.
    Break,
    /// In physical screen pixels.
    Button { button: MouseButton, down: bool, x: i32, y: i32 },
    Motion { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Watch {
    pub keys: bool,
    pub buttons: bool,
    pub motion: bool,
}

/// What the focused control is, checked just before a trigger expands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Field {
    pub password: bool,
    /// Text is selected, such as an address bar's inline completion after the caret.
    pub selection: bool,
}

/// Starts watching on a thread of its own, which ends at the first event after the receiver is
/// dropped. On Windows, input other programs inject (Notifi's own typing included) is left out.
pub fn observe(watch: Watch) -> Result<EventStream<Observed>> {
    #[cfg(windows)]
    let stream = crate::win_observe::observe(watch);
    #[cfg(target_os = "linux")]
    let stream = crate::x11_observe::observe(watch);
    #[cfg(not(any(windows, target_os = "linux")))]
    let stream = {
        let _ = watch;
        Err(crate::error::PlatformError::not_implemented(crate::caps::Feature::InputHook))
    };
    stream
}

/// The focused control, through UI Automation on Windows. X11 has no cheap way to tell, so there
/// it reports neither.
pub fn focused_field() -> Field {
    #[cfg(windows)]
    let field = crate::win_observe::focused_field();
    #[cfg(not(windows))]
    let field = Field::default();
    field
}
