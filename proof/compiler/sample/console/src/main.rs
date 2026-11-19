pub use instrument::measured as sample_value;
use port::{Panel, Readout};
struct Bridge;
struct Spare;
struct NeverInstalled;

fn sample() -> u8 {
    sample_value()
}

impl Readout for Bridge {
    fn read(&self) -> u8 {
        instrument::audit();
        sample()
    }
}

impl Readout for Spare {
    fn read(&self) -> u8 {
        33
    }
}

impl Readout for NeverInstalled {
    fn read(&self) -> u8 {
        44
    }
}

fn forward(panel: Panel<'_>) -> u8 {
    port::display(panel)
}

fn choose(adapted: bool) -> &'static dyn Readout {
    if adapted {
        &Bridge
    } else {
        &instrument::Factory
    }
}

#[cfg(feature = "calibration")]
fn calibrate() -> u8 {
    instrument::measured()
}

fn correlated(flag: bool) -> Option<u8> {
    let picked = choose(flag);
    if flag {
        None
    } else {
        Some(port::gated(picked))
    }
}

fn edit(slot: &mut &'static dyn Readout) {
    *slot = &Spare;
}

fn main() {
    let adapted = std::env::args().nth(1).as_deref() == Some("adapted");
    assert_eq!(correlated(adapted), if adapted { None } else { Some(11) });
    let mut slot = choose(adapted);
    edit(&mut slot);
    assert_eq!(port::after_edit(slot), 33);
    let panel = Panel {
        active: choose(adapted),
        spare: &Spare,
    };
    println!("{}", forward(panel));
    std::hint::black_box(port::generic(&NeverInstalled));
    std::hint::black_box(port::via_callback(sample));
    #[cfg(feature = "calibration")]
    std::hint::black_box(calibrate());
}
