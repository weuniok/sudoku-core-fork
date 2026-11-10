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
        if self.used >= self.limit || !tick() {
            self.exhausted = true;
            return false;
        }
        self.used += 1;
        true
    }
}
use std::cell::RefCell;

thread_local! {
    // A request stays synchronous on one native worker thread. Nested technique
    // searches share this account; normal solver/rating calls have no account.
    static REQUEST: RefCell<Option<(u32, u32, bool)>> = const { RefCell::new(None) };
}

pub(crate) struct RequestBudget(Option<(u32, u32, bool)>);
impl RequestBudget {
    pub fn enter(limit: u32) -> Self {
        Self(REQUEST.with(|slot| slot.replace(Some((0, limit, false)))))
    }
    pub fn usage(&self) -> (u32, bool) {
        REQUEST.with(|slot| {
            slot.borrow()
                .map(|(used, _, exhausted)| (used, exhausted))
                .unwrap_or_default()
        })
    }
}
impl Drop for RequestBudget {
    fn drop(&mut self) {
        REQUEST.with(|slot| slot.replace(self.0));
    }
}

pub(crate) fn tick() -> bool {
    REQUEST.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some((used, limit, exhausted)) = slot.as_mut() else {
            return true;
        };
        if *exhausted || *used >= *limit {
            *exhausted = true;
            return false;
        }
        *used += 1;
        true
    })
}

pub(crate) fn exhaust() {
    REQUEST.with(|slot| {
        if let Some((_, _, exhausted)) = slot.borrow_mut().as_mut() {
            *exhausted = true;
        }
    });
}
