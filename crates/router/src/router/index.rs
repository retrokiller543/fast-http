use crate::Pattern;

/// Candidate routes for each literal first segment, so a request never scans every route.
///
/// Each candidate list is ordered most specific first, and in registration order among equally
/// specific routes, so a route registered later still wins where it is more specific. The order
/// is settled while routes are added; a lookup only walks a list.
#[derive(Debug, Default)]
pub(super) struct Index {
    literal: Vec<(&'static str, Vec<u32>)>,
    dynamic: Vec<u32>,
}

impl Index {
    /// Records the route at `index`, whose patterns, its own included, `pattern_of` returns.
    ///
    /// # Panics
    ///
    /// When `index` does not fit in a `u32`, or when the route matches exactly the paths of a
    /// route recorded before it, which would leave one of the two unreachable.
    pub(super) fn push<'p>(&mut self, index: usize, pattern_of: impl Fn(usize) -> &'p Pattern) {
        let pattern = pattern_of(index);
        let position = u32::try_from(index).expect("a router holds at most u32::MAX routes");

        if let Some(earlier) = (0..index).find(|&earlier| pattern_of(earlier).same_shape(pattern)) {
            panic!(
                "route `{pattern}` matches the same paths as `{}`, registered before it, so one of them could never be reached; put the endpoints of both on one route",
                pattern_of(earlier)
            );
        }

        let Some(key) = pattern.first_literal() else {
            insert(&mut self.dynamic, position, &pattern_of);

            for (_, candidates) in &mut self.literal {
                insert(candidates, position, &pattern_of);
            }

            return;
        };

        match self.literal.binary_search_by_key(&key, |entry| entry.0) {
            Ok(found) => insert(&mut self.literal[found].1, position, &pattern_of),
            Err(found) => {
                let mut candidates = self.dynamic.clone();

                insert(&mut candidates, position, &pattern_of);
                self.literal.insert(found, (key, candidates));
            }
        }
    }

    /// The routes that can match a path whose first segment is `first`, most specific first.
    pub(super) fn candidates(&self, first: &str) -> &[u32] {
        match self.literal.binary_search_by_key(&first, |entry| entry.0) {
            Ok(position) => &self.literal[position].1,
            Err(_) => &self.dynamic,
        }
    }
}

/// Inserts `route` after every candidate at least as specific, keeping registration order among
/// equals.
fn insert<'p>(candidates: &mut Vec<u32>, route: u32, pattern_of: &impl Fn(usize) -> &'p Pattern) {
    let pattern = pattern_of(route as usize);
    let at = candidates.partition_point(|&candidate| {
        pattern_of(candidate as usize)
            .cmp_specificity(pattern)
            .is_le()
    });

    candidates.insert(at, route);
}
