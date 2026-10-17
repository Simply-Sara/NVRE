//! The double-click detector: a real double-click frames a reference, and
//! nothing else does.

use nvre_editor::viewport::ClickState;

#[test]
fn two_clicks_on_the_same_reference_are_a_double() {
    let mut c = ClickState::default();
    assert!(!c.register(0x1000), "the first click is never a double");
    assert!(c.register(0x1000), "the second, on the same reference, is");
}

#[test]
fn clicks_far_apart_are_two_singles() {
    let mut c = ClickState::default();
    assert!(!c.register(0x1000));
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert!(!c.register(0x1000));
}

#[test]
fn a_click_on_another_reference_resets_the_pair() {
    let mut c = ClickState::default();
    assert!(!c.register(0x1000));
    assert!(
        !c.register(0x2000),
        "a different reference is not the same target"
    );
}

#[test]
fn three_clicks_are_one_double_then_a_single() {
    let mut c = ClickState::default();
    let seen = [c.register(7), c.register(7), c.register(7)];
    assert_eq!(seen, [false, true, false], "a matched pair is consumed");
}
