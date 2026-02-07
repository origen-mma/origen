use ordered_float::OrderedFloat;
use std::collections::BTreeMap;

/// Temporal clustering using binary search
/// Based on SGN-LLAI's bisect approach
pub struct TemporalIndex {
    /// Map of t_0 → superevent_id
    times: BTreeMap<OrderedFloat<f64>, String>,
}

impl TemporalIndex {
    pub fn new() -> Self {
        Self {
            times: BTreeMap::new(),
        }
    }

    /// Add a superevent to the index
    pub fn insert(&mut self, t_0: f64, superevent_id: String) {
        self.times.insert(OrderedFloat(t_0), superevent_id);
    }

    /// Remove a superevent from the index
    pub fn remove(&mut self, t_0: f64) -> Option<String> {
        self.times.remove(&OrderedFloat(t_0))
    }

    /// Find superevents within a time window
    /// Returns all superevents where alert_time is within [t_start, t_end]
    pub fn find_in_window(&self, alert_time: f64, window_before: f64, window_after: f64) -> Vec<(f64, String)> {
        let search_start = alert_time + window_before; // window_before is negative
        let search_end = alert_time + window_after;

        let mut results = Vec::new();

        // Find all t_0 values where the superevent window overlaps with search window
        for (t_0, superevent_id) in self.times.range(
            OrderedFloat(search_start)..=OrderedFloat(search_end)
        ) {
            results.push((t_0.0, superevent_id.clone()));
        }

        results
    }

    /// Find closest superevent to a given time
    pub fn find_closest(&self, time: f64) -> Option<(f64, String)> {
        if self.times.is_empty() {
            return None;
        }

        let key = OrderedFloat(time);

        // Get the entry at or after this time
        let after = self.times.range(key..).next();

        // Get the entry before this time
        let before = self.times.range(..key).next_back();

        match (before, after) {
            (Some((t1, id1)), Some((t2, id2))) => {
                // Return closest
                if (time - t1.0).abs() < (time - t2.0).abs() {
                    Some((t1.0, id1.clone()))
                } else {
                    Some((t2.0, id2.clone()))
                }
            }
            (Some((t, id)), None) | (None, Some((t, id))) => Some((t.0, id.clone())),
            (None, None) => None,
        }
    }

    /// Remove superevents older than a cutoff time
    pub fn cleanup_old(&mut self, cutoff_time: f64) -> Vec<String> {
        let old_keys: Vec<_> = self
            .times
            .range(..OrderedFloat(cutoff_time))
            .map(|(t, _)| *t)
            .collect();

        let mut removed = Vec::new();
        for key in old_keys {
            if let Some(id) = self.times.remove(&key) {
                removed.push(id);
            }
        }

        removed
    }

    /// Get number of tracked superevents
    pub fn len(&self) -> usize {
        self.times.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.times.is_empty()
    }
}

impl Default for TemporalIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temporal_index() {
        let mut index = TemporalIndex::new();

        // Add some superevents
        index.insert(1000.0, "S1".to_string());
        index.insert(1005.0, "S2".to_string());
        index.insert(1010.0, "S3".to_string());

        assert_eq!(index.len(), 3);

        // Find within window
        let results = index.find_in_window(1002.0, -5.0, 10.0);
        assert_eq!(results.len(), 3); // All within [-5, +10] of 1002

        // Find closest
        let closest = index.find_closest(1007.0);
        assert_eq!(closest, Some((1005.0, "S2".to_string())));

        // Cleanup old
        let removed = index.cleanup_old(1006.0);
        assert_eq!(removed.len(), 2); // S1 and S2
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn test_find_in_window_raven_params() {
        let mut index = TemporalIndex::new();

        // GW event at t=1000
        index.insert(1000.0, "S1".to_string());

        // Optical alert 1 hour later
        let optical_time = 1000.0 + 3600.0;

        // Search with RAVEN parameters: look back 1 day, forward 1s
        // From optical perspective: GW should be between [optical - 86400, optical + 1]
        let results = index.find_in_window(optical_time, -86400.0, 1.0);

        // Should find S1 since GW is 1 hour before optical (within 1 day window)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "S1");
    }
}
