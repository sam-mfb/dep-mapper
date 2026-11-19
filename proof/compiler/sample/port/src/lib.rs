pub trait Readout {
    fn read(&self) -> u8;
}

pub struct Panel<'a> {
    pub active: &'a dyn Readout,
    pub spare: &'a dyn Readout,
}

pub fn display(panel: Panel<'_>) -> u8 {
    panel.active.read()
}

pub fn generic<T: Readout>(value: &T) -> u8 {
    value.read()
}

pub fn via_callback(callback: fn() -> u8) -> u8 {
    callback()
}

pub fn gated(value: &dyn Readout) -> u8 {
    value.read()
}
pub fn after_edit(value: &dyn Readout) -> u8 {
    value.read()
}
