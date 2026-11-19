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

fn exercise(left: bool, right: bool) {
    if left && !left {
        std::hint::black_box(port::contradictory(&Spare));
    }
    let source = choose(left);
    if right {
        assert_eq!(port::independent(source), if left { 22 } else { 11 });
    }
    let other = choose(!right);
    if right {
        assert_eq!(port::inverted(other), 11);
    }
    let socket = if left {
        port::Socket::Active(&Bridge)
    } else {
        port::Socket::Spare(&Spare)
    };
    assert_eq!(
        port::active_only(socket),
        if left { Some(22) } else { None }
    );
}

fn main() {
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Poll, Waker},
    };
    for left in [false, true] {
        for right in [false, true] {
            exercise(left, right);
        }
    }
    let configured: &dyn Readout = if cfg!(feature = "calibration") {
        &Bridge
    } else {
        &Spare
    };
    let expected = if cfg!(feature = "calibration") {
        22
    } else {
        33
    };
    assert_eq!(port::build_choice(configured), expected);
    assert_eq!(
        port::redirected(port::Redirect {
            unused: &Bridge,
            selected: &Spare
        }),
        33
    );
    assert_eq!(port::from_generic(port::erased(&Bridge)), 22);
    std::hint::black_box(port::erased(&Spare));
    let input = Arc::new(Bridge);
    assert_eq!(port::shared(input), 22);
    let mut deferred = std::pin::pin!(port::deferred(&instrument::Factory));
    assert_eq!(
        deferred
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(11)
    );

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
