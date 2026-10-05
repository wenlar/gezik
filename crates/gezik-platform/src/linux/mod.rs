//! Linux (and other X11/Wayland systems): the system clipboard and drag and drop.

#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod uri;
