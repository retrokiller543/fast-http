/// Candidate routes for each literal first segment, so a request never scans every route.
///
/// Grows one route at a time; candidate lists stay in declaration order because each new route
/// has the highest index so far.
#[derive(Debug, Default)]
pub(super) struct Index {
    literal: Vec<(&'static str, Vec<u32>)>,
    dynamic: Vec<u32>,
}

impl Index {
    /// Records the route at `index`, whose pattern starts with `first` when that is a literal.
    ///
    /// # Panics
    ///
    /// When `index` does not fit in a `u32`.
    pub(super) fn push(&mut self, first: Option<&'static str>, index: usize) {
        let index = u32::try_from(index).expect("a router holds at most u32::MAX routes");

        let Some(key) = first else {
            self.dynamic.push(index);

            for (_, candidates) in &mut self.literal {
                candidates.push(index);
            }

            return;
        };

        match self.literal.binary_search_by_key(&key, |entry| entry.0) {
            Ok(position) => self.literal[position].1.push(index),
            Err(position) => {
                let mut candidates = self.dynamic.clone();

                candidates.push(index);
                self.literal.insert(position, (key, candidates));
            }
        }
    }

    /// The routes that can match a path whose first segment is `first`, in declaration order.
    pub(super) fn candidates(&self, first: &str) -> &[u32] {
        match self.literal.binary_search_by_key(&first, |entry| entry.0) {
            Ok(position) => &self.literal[position].1,
            Err(_) => &self.dynamic,
        }
    }
}
