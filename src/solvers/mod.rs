use std::rc::Rc;

pub mod dfs;

#[derive(Default)]
struct TrackedCell {
    coord: (u8, u8),
    parent: Option<Rc<TrackedCell>>,
    distance_cost: usize,
    heuristc_cost: usize,
}
