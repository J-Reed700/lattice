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

    /// Which owners are shown, and with which places of this material. `needs[i]` are the places (positions
    /// in this material) owner `i` needs. Owners are taken in order: one is kept when what it needs, and no
    /// owner kept before it brought, fits in what is left of `room`; one that does not fit is passed over
    /// and later owners are still tried. When none is kept whole, the first owner is kept alone with the
    /// opening of its own needs that fits (possibly none). Each kept owner comes back with the places it
    /// is shown with.
    pub fn for_owners(&self, room: usize, needs: &[Vec<usize>]) -> Vec<(usize, Vec<usize>)> {
        let mut brought: Vec<usize> = Vec::new();
        let mut left = room;
        let mut kept: Vec<(usize, Vec<usize>)> = Vec::new();
        for (owner, wanted) in needs.iter().enumerate() {
            let places = self.places(wanted);
            let mut fresh: Vec<usize> = Vec::new();
            for place in &places {
                if !brought.contains(place) {
                    fresh.push(*place);
                }
            }
            let cost: usize = fresh.iter().map(|place| self.cost_of(*place)).sum();
            if let Some(rest) = left.checked_sub(cost) {
                left = rest;
                brought.extend(fresh);
                kept.push((owner, places));
            }
        }
        if kept.is_empty() {
            if let Some(first) = needs.first() {
                let mut left = room;
                let opening: Vec<usize> = self
                    .places(first)
                    .into_iter()
                    .take_while(|place| match left.checked_sub(self.cost_of(*place)) {
                        Some(rest) => {
                            left = rest;
                            true
                        }
                        None => false,
                    })
                    .collect();
                kept.push((0, opening));
            }
        }
        kept
    }

    /// The places of `wanted` this material has, each once, in the order given.
    fn places(&self, wanted: &[usize]) -> Vec<usize> {
        let mut places: Vec<usize> = Vec::new();
        for place in wanted {
            if *place < self.items.len() && !places.contains(place) {
                places.push(*place);
            }
        }
        places
    }

    fn cost_of(&self, place: usize) -> usize {
        self.items.get(place).map_or(0, |(_, cost)| *cost)
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
    fn material_two_owners_share_is_counted_once() {
        let packed = costed(&[4, 3, 2]);
        let needs = vec![vec![0, 1], vec![1, 2], vec![0, 2]];
        assert_eq!(
            packed.for_owners(9, &needs),
            [(0, vec![0, 1]), (1, vec![1, 2]), (2, vec![0, 2])]
        );
        assert_eq!(
            packed.for_owners(7, &needs),
            [(0, vec![0, 1])]
        );
    }

    #[test]
    fn an_owner_that_does_not_fit_is_passed_over_and_later_ones_are_tried() {
        let packed = costed(&[2, 9, 1]);
        let needs = vec![vec![0], vec![1], vec![2]];
        assert_eq!(
            packed.for_owners(4, &needs),
            [(0, vec![0]), (2, vec![2])]
        );
    }

    #[test]
    fn an_owner_with_no_needs_always_fits() {
        let packed = costed(&[5]);
        assert_eq!(
            packed.for_owners(0, &[vec![0], vec![]]),
            [(1, vec![])]
        );
    }

    #[test]
    fn when_no_owner_fits_the_first_is_kept_with_the_opening_of_its_needs() {
        let packed = costed(&[3, 4, 5]);
        let needs = vec![vec![0, 1, 2], vec![2]];
        assert_eq!(packed.for_owners(4, &needs), [(0, vec![0])]);
        assert_eq!(packed.for_owners(2, &needs), [(0, vec![])]);
    }

    #[test]
    fn no_owners_keep_nothing() {
        assert!(costed(&[1]).for_owners(10, &[]).is_empty());
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
