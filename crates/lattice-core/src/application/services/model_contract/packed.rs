//! Material a request may not hold whole.

use serde::{Deserialize, Serialize};

/// Things in the order of their worth, each with the tokens showing it takes.
/// A contract's input holds its material this way and cuts it to a room in
/// [`Answered::within`](super::ask::Answered::within); the driver finds the room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Packed<T> {
    items: Vec<(T, usize)>,
}

impl<T> Default for Packed<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Packed<T> {
    /// `items`, the most wanted first; `cost` is the tokens one takes when shown.
    pub fn new(items: Vec<T>, cost: impl Fn(&T) -> usize) -> Self {
        let items = items
            .into_iter()
            .map(|item| {
                let cost = cost(&item);
                (item, cost)
            })
            .collect();
        Self { items }
    }

    /// The opening of the material that `room` tokens hold.
    pub fn within(&self, room: usize) -> Self
    where
        T: Clone,
    {
        let mut left = room;
        let items = self
            .items
            .iter()
            .take_while(|(_, cost)| match left.checked_sub(*cost) {
                Some(rest) => {
                    left = rest;
                    true
                }
                None => false,
            })
            .cloned()
            .collect();
        Self { items }
    }

    /// The things held, the most wanted first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter().map(|(item, _)| item)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn costed(costs: &[usize]) -> Packed<usize> {
        Packed::new((0..costs.len()).collect(), |at| costs[*at])
    }

    fn held(packed: &Packed<usize>) -> Vec<usize> {
        packed.iter().copied().collect()
    }

    #[test]
    fn the_opening_that_fits_is_held() {
        let packed = costed(&[3, 4, 5]);
        assert_eq!(held(&packed.within(7)), [0, 1]);
        assert_eq!(held(&packed.within(12)), [0, 1, 2]);
    }

    #[test]
    fn nothing_after_the_first_that_does_not_fit_is_held() {
        let packed = costed(&[3, 9, 1]);
        let opening = packed.within(5);
        assert_eq!(held(&opening), [0]);
        assert_eq!(opening.len(), 1);
    }

    #[test]
    fn no_room_holds_nothing() {
        assert!(costed(&[3, 4]).within(0).is_empty());
        assert_eq!(held(&costed(&[0, 0, 2]).within(0)), [0, 1]);
        assert_eq!(held(&costed(&[0]).within(5)), [0]);
    }

    #[test]
    fn the_order_given_is_kept() {
        let packed = Packed::new(vec!["tide", "kiln", "quay"], |_| 1);
        assert_eq!(
            packed.iter().copied().collect::<Vec<_>>(),
            ["tide", "kiln", "quay"]
        );
        assert_eq!(packed.len(), 3);
    }
}
