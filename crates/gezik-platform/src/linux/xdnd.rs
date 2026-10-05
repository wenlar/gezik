//! The XDND protocol's client messages (version 5, freedesktop.org), as their five 32-bit
//! words. Atoms are plain numbers here, so this compiles and is tested everywhere.

/// The highest XDND version Gezik speaks.
pub const VERSION: u32 = 5;

/// `XdndEnter` from `source`: version, whether more than three types follow in
/// `XdndTypeList`, and the first three types.
pub fn enter(source: u32, version: u32, types: &[u32]) -> [u32; 5] {
    let first = |i: usize| types.get(i).copied().unwrap_or(0);
    [source, (version << 24) | u32::from(types.len() > 3), first(0), first(1), first(2)]
}

/// What an `XdndEnter` says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enter {
    pub source: u32,
    pub version: u32,
    /// The types are in the source's `XdndTypeList` property, not here.
    pub more_types: bool,
    pub types: Vec<u32>,
}

pub fn parse_enter(data: [u32; 5]) -> Enter {
    Enter {
        source: data[0],
        version: data[1] >> 24,
        more_types: data[1] & 1 != 0,
        types: data[2..].iter().copied().filter(|&atom| atom != 0).collect(),
    }
}

/// `XdndPosition`: the pointer at root (`x`, `y`), the time, and the action the source suggests.
pub fn position(source: u32, x: i16, y: i16, time: u32, action: u32) -> [u32; 5] {
    [source, 0, (u32::from(x as u16) << 16) | u32::from(y as u16), time, action]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub source: u32,
    pub x: i16,
    pub y: i16,
    pub time: u32,
    pub action: u32,
}

pub fn parse_position(data: [u32; 5]) -> Position {
    Position {
        source: data[0],
        x: (data[2] >> 16) as u16 as i16,
        y: (data[2] & 0xFFFF) as u16 as i16,
        time: data[3],
        action: data[4],
    }
}

/// `XdndStatus` from `target`: whether it would take a drop and with which action. It always
/// asks for further positions (no "silent" rectangle).
pub fn status(target: u32, accept: bool, action: u32) -> [u32; 5] {
    [target, u32::from(accept) | 0b10, 0, 0, if accept { action } else { 0 }]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub target: u32,
    pub accept: bool,
    pub action: u32,
}

pub fn parse_status(data: [u32; 5]) -> Status {
    let accept = data[1] & 1 != 0;
    Status { target: data[0], accept, action: if accept { data[4] } else { 0 } }
}

pub fn leave(source: u32) -> [u32; 5] {
    [source, 0, 0, 0, 0]
}

pub fn drop(source: u32, time: u32) -> [u32; 5] {
    [source, 0, time, 0, 0]
}

/// `XdndFinished` from `target`: whether the drop was taken, and the action done.
pub fn finished(target: u32, accepted: bool, action: u32) -> [u32; 5] {
    [target, u32::from(accepted), if accepted { action } else { 0 }, 0, 0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_carries_the_version_and_up_to_three_types() {
        let data = enter(7, 5, &[11, 12]);
        assert_eq!(data, [7, 5 << 24, 11, 12, 0]);
        assert_eq!(parse_enter(data), Enter { source: 7, version: 5, more_types: false, types: vec![11, 12] });
        let many = enter(7, 5, &[1, 2, 3, 4]);
        assert_eq!(many[1], (5 << 24) | 1, "more than three: the list is a property");
        assert_eq!(parse_enter(many), Enter { source: 7, version: 5, more_types: true, types: vec![1, 2, 3] });
    }

    #[test]
    fn positions_pack_root_coordinates() {
        let data = position(7, 300, 1200, 99, 42);
        assert_eq!(data, [7, 0, (300 << 16) | 1200, 99, 42]);
        assert_eq!(parse_position(data), Position { source: 7, x: 300, y: 1200, time: 99, action: 42 });
        let negative = parse_position(position(7, -5, -6, 0, 0));
        assert_eq!((negative.x, negative.y), (-5, -6), "monitors left of or above the first");
    }

    #[test]
    fn status_says_accept_and_asks_for_more_positions() {
        let data = status(9, true, 42);
        assert_eq!(data, [9, 0b11, 0, 0, 42]);
        assert_eq!(parse_status(data), Status { target: 9, accept: true, action: 42 });
        assert_eq!(parse_status(status(9, false, 42)), Status { target: 9, accept: false, action: 0 });
    }

    #[test]
    fn finished_names_the_action_only_when_taken() {
        assert_eq!(finished(9, true, 42), [9, 1, 42, 0, 0]);
        assert_eq!(finished(9, false, 42), [9, 0, 0, 0, 0]);
    }
}
