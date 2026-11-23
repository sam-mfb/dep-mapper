pub struct Factory;
impl port::Readout for Factory {
    fn read(&self) -> u8 {
        11
    }
}

pub fn measured() -> u8 {
    22
}
pub fn audit() {
    std::hint::black_box(());
}
