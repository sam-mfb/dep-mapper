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

pub async fn deferred(value: &dyn Readout) -> u8 {
    std::future::ready(()).await;
    value.read()
}

pub fn shared(value: std::sync::Arc<dyn Readout>) -> u8 {
    value.clone().as_ref().read()
}

pub fn independent(value: &dyn Readout) -> u8 {
    value.read()
}
pub fn inverted(value: &dyn Readout) -> u8 {
    value.read()
}

pub enum Socket<'a> {
    Active(&'a dyn Readout),
    Spare(&'a dyn Readout),
}
pub fn active_only(socket: Socket<'_>) -> Option<u8> {
    match socket {
        Socket::Active(value) => Some(value.read()),
        Socket::Spare(_) => None,
    }
}

pub fn build_choice(value: &dyn Readout) -> u8 {
    value.read()
}

pub struct Redirect<'a> {
    pub unused: &'a dyn Readout,
    pub selected: &'a dyn Readout,
}
impl<'a> std::ops::Deref for Redirect<'a> {
    type Target = dyn Readout + 'a;
    fn deref(&self) -> &Self::Target {
        self.selected
    }
}
pub fn redirected(wrapper: Redirect<'_>) -> u8 {
    wrapper.read()
}

pub fn erased<T: Readout>(value: &T) -> &dyn Readout {
    value
}
pub fn from_generic(value: &dyn Readout) -> u8 {
    value.read()
}

pub fn contradictory(value: &dyn Readout) -> u8 {
    value.read()
}
