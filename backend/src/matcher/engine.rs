//! Combination search engine.
//!
//! Replaces the Python brute-force `itertools.combinations` enumeration with a
//! depth-first branch-and-bound over amounts sorted descending. Because every
//! invoice amount is strictly positive, partial sums grow monotonically, which
//! makes two cheap prunes exact (they never discard valid solutions):
//!
//! 1. sum prune   – once the running total exceeds `target + tolerance`, adding
//!                  more invoices can never re-enter the window.
//! 2. reach prune – if even the `remaining_slots` largest untouched amounts
//!                  cannot lift the running total to `target - tolerance`,
//!                  this branch is dead.
//!
//! The result set is therefore identical to brute force; only the traversal
//! order differs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct InvoiceRow {
    pub invoice_no: String,
    pub date_label: String,
    pub amount: i64,
}

#[derive(Debug, Clone)]
pub struct Combo {
    pub indices: Vec<usize>,
    pub total: i64,
}

/// Cooperative deadline checked every few thousand explored nodes so long
/// searches abort cleanly instead of blocking the worker thread forever.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    at: Instant,
}

impl Deadline {
    pub fn after(seconds: u64) -> Self {
        Deadline {
            at: Instant::now() + std::time::Duration::from_secs(seconds),
        }
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.at
    }
}

#[derive(Debug)]
pub enum SearchError {
    Timeout,
}

const CHECK_EVERY: u64 = 4096;

struct Search<'a> {
    rows: &'a [InvoiceRow],
    sorted: Vec<usize>,
    /// suffix sums of the largest `k` remaining amounts, for reach pruning
    reach: Vec<i64>,
    target: i64,
    tol: i64,
    max_invoices: usize,
    deadline: Deadline,
    nodes: AtomicU64,
    out: Vec<Combo>,
}

impl<'a> Search<'a> {
    /// Sum of the `slots` largest amounts among sorted[pos..] (0 if slots == 0
    /// or no items remain).
    fn reachable(&self, pos: usize, slots: usize) -> i64 {
        if slots == 0 || pos >= self.sorted.len() {
            return 0;
        }
        let end = (pos + slots).min(self.reach.len() - 1);
        self.reach[pos] - self.reach[end]
    }

    fn check_deadline(&self) -> Result<(), SearchError> {
        let n = self.nodes.fetch_add(1, Ordering::Relaxed);
        if n % CHECK_EVERY == 0 && self.deadline.expired() {
            return Err(SearchError::Timeout);
        }
        Ok(())
    }

    fn dfs(
        &mut self,
        pos: usize,
        depth: usize,
        sum: i64,
        chosen: &mut Vec<usize>,
    ) -> Result<(), SearchError> {
        self.check_deadline()?;

        if depth > 0 && (sum - self.target).abs() <= self.tol {
            // Restore original sheet order inside the combo for output parity
            // with the legacy itertools implementation.
            let mut indices = chosen.clone();
            indices.sort_unstable();
            self.out.push(Combo {
                indices,
                total: sum,
            });
        }

        if pos >= self.sorted.len() || depth >= self.max_invoices {
            return Ok(());
        }

        for idx in pos..self.sorted.len() {
            let amount = self.rows[self.sorted[idx]].amount;
            let next_sum = sum + amount;

            // Prune 1: this invoice alone overflows the window. The list is
            // descending, so smaller invoices further down may still fit —
            // skip, do not stop.
            if next_sum > self.target + self.tol {
                continue;
            }
            // Prune 2: even taking every remaining (largest-first) invoice
            // cannot reach the bottom of the window. Later invoices are all
            // smaller, so nothing beyond this point can help either — stop.
            let best_future = self.reachable(idx + 1, self.max_invoices - depth - 1);
            if next_sum + best_future < self.target - self.tol {
                break;
            }

            chosen.push(self.sorted[idx]);
            self.dfs(idx + 1, depth + 1, next_sum, chosen)?;
            chosen.pop();
        }

        Ok(())
    }
}

/// Finds all combinations of size 1..=max_invoices whose total lies within
/// `[target - tolerance, target + tolerance]`.
pub fn find_combinations(
    rows: &[InvoiceRow],
    target: i64,
    tolerance: i64,
    max_invoices: usize,
    deadline: Deadline,
) -> Result<Vec<Combo>, SearchError> {
    let mut sorted: Vec<usize> = (0..rows.len()).collect();
    sorted.sort_by(|a, b| rows[*b].amount.cmp(&rows[*a].amount));

    // reach[i] = sum of sorted[i..] capped at k elements per query; we store the
    // plain suffix sum of ALL remaining items and cap the slice at query time.
    let mut reach = vec![0i64; rows.len() + 1];
    for i in (0..rows.len()).rev() {
        reach[i] = reach[i + 1] + rows[sorted[i]].amount;
    }

    let mut search = Search {
        rows,
        sorted,
        reach,
        target,
        tol: tolerance,
        max_invoices: max_invoices.max(1),
        deadline,
        nodes: AtomicU64::new(0),
        out: Vec::new(),
    };

    let mut chosen = Vec::with_capacity(search.max_invoices);
    search.dfs(0, 0, 0, &mut chosen)?;

    Ok(std::mem::take(&mut search.out))
}

/// Naive exhaustive reference used by property tests to prove the pruned
/// search returns exactly the same solution set.
#[cfg(test)]
pub fn brute_force_reference(
    rows: &[InvoiceRow],
    target: i64,
    tolerance: i64,
    max_invoices: usize,
) -> Vec<Combo> {
    let mut out = Vec::new();
    let n = rows.len();
    let limit = max_invoices.min(n);
    // Gray-code style subset enumeration up to size limit.
    for mask in 1u128..(1u128 << n.min(63)) {
        let bits = mask.count_ones() as usize;
        if bits == 0 || bits > limit {
            continue;
        }
        let mut indices = Vec::with_capacity(bits);
        let mut total = 0i64;
        for i in 0..n {
            if mask & (1 << i) != 0 {
                indices.push(i);
                total += rows[i].amount;
            }
        }
        if (total - target).abs() <= tolerance {
            out.push(Combo { indices, total });
        }
    }
    out
}

#[cfg(test)]
fn canonical(combos: &[Combo]) -> Vec<(usize, i64, Vec<usize>)> {
    let mut keys: Vec<_> = combos
        .iter()
        .map(|c| (c.indices.len(), c.total, {
            let mut idx = c.indices.clone();
            idx.sort_unstable();
            idx
        }))
        .collect();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(amount: i64) -> InvoiceRow {
        InvoiceRow {
            invoice_no: format!("INV-{amount}"),
            date_label: "01/01/2024".into(),
            amount,
        }
    }

    fn assert_same_set(a: &[Combo], b: &[Combo], context: &str) {
        assert_eq!(canonical(a), canonical(b), "mismatch: {context}");
    }

    #[test]
    fn matches_brute_force_on_random_inputs() {
        // Deterministic LCG so failures are reproducible.
        let mut seed: u64 = 42;
        let mut rng = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as i64
        };

        for case in 0..200u32 {
            let n = (rng().rem_euclid(12) + 1) as usize;
            let rows: Vec<InvoiceRow> = (0..n)
                .map(|_| row(rng().rem_euclid(500_000) + 100))
                .collect();
            let target = rng().rem_euclid(1_000_000);
            let tol = rng().rem_euclid(20_000);
            let k = (rng().rem_euclid(4) + 1) as usize;

            let fast =
                find_combinations(&rows, target, tol, k, Deadline::after(30)).expect("no timeout");
            let slow = brute_force_reference(&rows, target, tol, k);

            assert_same_set(
                &fast,
                &slow,
                &format!("case={case} n={n} target={target} tol={tol} k={k}"),
            );
        }
    }

    #[test]
    fn finds_exact_match() {
        let rows = vec![row(100), row(200), row(300)];
        let combos = find_combinations(&rows, 300, 0, 3, Deadline::after(10)).unwrap();
        let totals: Vec<i64> = combos.iter().map(|c| c.total).collect();
        assert!(totals.contains(&300));
        // single invoice 300 and combination 100+200 both match
        assert_eq!(combos.len(), 2);
    }

    #[test]
    fn respects_tolerance_window() {
        let rows = vec![row(1000), row(1010)];
        let combos = find_combinations(&rows, 1005, 10, 2, Deadline::after(10)).unwrap();
        assert_eq!(combos.len(), 2); // both singles fall inside ±10 of 1005
    }

    #[test]
    fn caps_combination_size() {
        let rows = vec![row(100), row(100), row(100)];
        let combos = find_combinations(&rows, 300, 0, 2, Deadline::after(10)).unwrap();
        assert!(combos.iter().all(|c| c.indices.len() <= 2));
    }

    #[test]
    fn times_out_when_deadline_passed() {
        let rows: Vec<InvoiceRow> = (0..40).map(|i| row((i as i64 + 1) * 1000)).collect();
        let deadline = Deadline {
            at: Instant::now() - std::time::Duration::from_secs(1),
        };
        let result = find_combinations(&rows, 999_999_999, 0, 5, deadline);
        assert!(matches!(result, Err(SearchError::Timeout)));
    }
}
