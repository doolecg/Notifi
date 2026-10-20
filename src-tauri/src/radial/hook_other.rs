//! No pointer hooks off Windows yet: the wheel opens from its shortcut and closes from its window.

use std::sync::mpsc::Sender;

use super::Event;

pub fn start(_tx: Sender<Event>) {}

pub fn set_button(_button: u32) {}

pub fn set_open(_ring: Option<(i32, i32, i32)>) {}
