//! The simulation says which sources changed, so that whoever draws the world need not
//! compare all of it.
use sim::SimState;

#[test]
fn a_reader_too_far_behind_is_told_to_look_at_everything() {
    let mut s = SimState::new(1);
    let start = s.resources.changes().count();
    let id = s.resources.add(sim::resources::ResourceKind::Plant, (3, 3), 4.0);
    assert_eq!(s.resources.changes().since(start).unwrap().collect::<Vec<_>>(), vec![id]);
    for _ in 0..10_000 {
        s.resources.add(sim::resources::ResourceKind::Plant, (4, 4), 4.0);
    }
    assert!(s.resources.changes().since(start).is_none());
    assert!(s.resources.changes().since(s.resources.changes().count() + 1).is_none());

    // A guess is read with the rest, and kept apart to be looked at again.
    let settled = s.resources.changes().count();
    s.guess_ahead();
    let guess = s.resources.add(sim::resources::ResourceKind::Plant, (5, 5), 4.0);
    assert_eq!(s.resources.changes().count(), settled);
    assert_eq!(s.resources.changes().since(settled).unwrap().collect::<Vec<_>>(), vec![guess]);
    assert_eq!(s.resources.changes().guesses().collect::<Vec<_>>(), vec![guess]);
}
