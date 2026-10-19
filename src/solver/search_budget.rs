/// Deterministic work accounting for interactive searches. Legacy solver calls
/// use an unlimited budget and retain their existing search scope.
pub(crate) struct SearchBudget {
    pub used: u32,
    limit: u32,
    pub exhausted: bool,
}

impl SearchBudget {
    pub fn new(limit: u32) -> Self {
        Self {
            used: 0,
            limit,
            exhausted: false,
        }
    }

    pub fn visit(&mut self) -> bool {
        if self.used >= self.limit {
            self.exhausted = true;
            return false;
        }
        self.used += 1;
        true
    }
}
