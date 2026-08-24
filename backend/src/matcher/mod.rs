//! Orchestration: load workbook once, search combinations for every target,
//! write a single result workbook. Port of `find_invoice_combinations_for_targets`.

pub mod engine;
pub mod loader;
pub mod writer;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::matcher::engine::{
    find_combinations, Combo, Deadline, InvoiceRow,
};
use crate::matcher::loader::{load_grouped, LoaderError};
use crate::matcher::writer::{build_output_path, commas, write_results, ResultRow};

#[derive(Debug)]
pub enum MatcherError {
    Loader(LoaderError),
    Timeout,
    /// Node budget exhausted — parameter space too wide to finish promptly.
    BudgetExceeded,
    Write(String),
}

pub struct RunOutcome {
    pub output_file: Option<PathBuf>,
    pub total_rows: usize,
    /// True when the search stopped early because the result cap was hit;
    /// the produced file contains a partial set of matches.
    pub truncated: bool,
}

fn row_to_result(
    combo: &Combo,
    customer: &str,
    target_label: String,
    group_rows: &[InvoiceRow],
) -> ResultRow {
    let invoices: Vec<&str> = combo
        .indices
        .iter()
        .map(|&i| group_rows[i].invoice_no.as_str())
        .collect();
    let dates: Vec<&str> = combo
        .indices
        .iter()
        .map(|&i| group_rows[i].date_label.as_str())
        .collect();
    let amounts: Vec<String> = combo
        .indices
        .iter()
        .map(|&i| format!("Rp{}", commas(group_rows[i].amount)))
        .collect();

    ResultRow {
        target: target_label,
        customer: customer.to_string(),
        invoice_count: combo.indices.len(),
        invoices: invoices.join(", "),
        dates: dates.join(", "),
        amounts: amounts.join(", "),
        total: combo.total,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_for_targets(
    input: &Path,
    targets: &[i64],
    tolerance: i64,
    max_invoices: usize,
    output_dir: &Path,
    timeout: Duration,
    node_budget: u64,
    result_cap: usize,
) -> Result<RunOutcome, MatcherError> {
    let grouped = load_grouped(input).map_err(MatcherError::Loader)?;

    let deadline = Deadline::after(timeout.as_secs());
    let max_invoices = max_invoices.max(1);
    let mut all_rows: Vec<ResultRow> = Vec::new();
    let mut truncated = false;

    'targets: for &target in targets {
        let target_label = format!("Rp{}", commas(target));
        for (customer, group_rows) in &grouped.groups {
            let search =
                find_combinations(
                    group_rows,
                    target,
                    tolerance,
                    max_invoices,
                    deadline,
                    node_budget,
                    result_cap.saturating_sub(all_rows.len()).max(1),
                )
                .map_err(|err| match err {
                    engine::SearchError::Timeout => MatcherError::Timeout,
                    engine::SearchError::BudgetExceeded => MatcherError::BudgetExceeded,
                })?;

            all_rows.extend(
                search.combos.iter().map(|combo| {
                    row_to_result(combo, customer, target_label.clone(), group_rows)
                }),
            );

            if search.truncated || all_rows.len() >= result_cap {
                truncated = true;
                break 'targets;
            }
        }
    }

    if all_rows.is_empty() {
        return Ok(RunOutcome {
            output_file: None,
            total_rows: 0,
            truncated,
        });
    }

    let output_file = build_output_path(output_dir);
    write_results(&output_file, &all_rows).map_err(|err| MatcherError::Write(err.to_string()))?;

    Ok(RunOutcome {
        output_file: Some(output_file),
        total_rows: all_rows.len(),
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::loader::format_date_label;
    use chrono::NaiveDate;

    /// End-to-end: build a synthetic .xlsx (same layout the frontend users
    /// upload), run the full pipeline, and verify the result workbook.
    #[test]
    fn end_to_end_synthetic_workbook() {
        let date = format_date_label(NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());

        // Write an input workbook resembling the real AR report.
        let mut wb = rust_xlsxwriter::Workbook::new();
        let sheet = wb.add_worksheet();
        // Row 0 is noise so header detection must skip it.
        sheet.write(0, 0, "Laporan Piutang Januari").unwrap();
        for (col, title) in
            ["Nama Pelanggan", "No. Faktur", "Tgl. Faktur", "Total"].iter().enumerate()
        {
            sheet.write(1, col as u16, *title).unwrap();
        }
        let rows: [(&str, &str, i64); 5] = [
            ("Toko A", "INV-001", 1_000_000),
            ("Toko A", "INV-002", 2_000_000),
            ("Toko A", "INV-003", 3_500_000),
            ("Toko B", "INV-004", 750_000),
            ("Toko B", "INV-005", 4_250_000),
        ];
        for (i, (customer, invoice, amount)) in rows.iter().enumerate() {
            let r = (i + 2) as u32;
            sheet.write(r, 0, *customer).unwrap();
            sheet.write(r, 1, *invoice).unwrap();
            sheet.write(r, 2, date.as_str()).unwrap();
            sheet.write(r, 3, *amount).unwrap();
        }

        let dir = std::env::temp_dir().join("ar_vanila_test");
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("input.xlsx");
        wb.save(&input).unwrap();

        let outcome = run_for_targets(
            &input,
            &[3_000_000],
            0,
            5,
            &dir,
            Duration::from_secs(30),
            u64::MAX,
            usize::MAX,
        )
        .expect("matcher should succeed");

        assert_eq!(outcome.total_rows, 1);
        let output = outcome.output_file.expect("result file");

        // Verify the produced workbook contents.
        use calamine::Reader;
        let mut reader: calamine::Xlsx<_> =
            calamine::open_workbook(&output).unwrap();
        let range = reader.worksheet_range_at(0).unwrap().unwrap();

        let get = |r: u32, c: u32| -> String {
            range
                .get_value((r, c))
                .map(|d| d.to_string())
                .unwrap_or_default()
        };
        assert_eq!(get(0, 0), "Target");
        assert_eq!(get(1, 0), "Rp3,000,000");
        assert_eq!(get(1, 1), "Toko A");
        assert_eq!(get(1, 2), "2");
        assert_eq!(get(1, 3), "INV-001, INV-002"); // 1M + 2M = 3M exact

        std::fs::remove_file(&input).ok();
        std::fs::remove_file(&output).ok();
    }

    /// Performance / parity benchmark. Defaults to `target/perf_big.xlsx`;
    /// override via env vars:
    ///   PERF_FILE=<path.xlsx> PERF_TARGETS=a,b,c PERF_TOL=500 PERF_OUTDIR=target/rs_out
    /// Run:
    ///   cargo test --release perf_benchmark -- --ignored --nocapture
    #[test]
    #[ignore]
    fn perf_benchmark() {
        let path = PathBuf::from(
            std::env::var("PERF_FILE").unwrap_or_else(|_| "target/perf_big.xlsx".into()),
        );
        if !path.exists() {
            eprintln!("skipping: {} not found", path.display());
            return;
        }
        let targets: Vec<i64> = std::env::var("PERF_TARGETS")
            .unwrap_or_else(|_| "7500000".into())
            .split(',')
            .map(|t| t.trim().parse().expect("bad target"))
            .collect();
        let tolerance: i64 = std::env::var("PERF_TOL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5_000);
        let out_dir = std::env::var("PERF_OUTDIR").unwrap_or_else(|_| "target".into());
        std::fs::create_dir_all(&out_dir).expect("failed to create output dir");

        let started = std::time::Instant::now();
        let outcome = run_for_targets(
            &path,
            &targets,
            tolerance,
            5,
            Path::new(&out_dir),
            Duration::from_secs(600),
            u64::MAX,
            usize::MAX,
        )
        .expect("matcher should succeed");
        let elapsed = started.elapsed();
        eprintln!(
            "RUST rows={} time={:.2}s",
            outcome.total_rows,
            elapsed.as_secs_f64()
        );
        if let Some(file) = outcome.output_file {
            eprintln!("OUTPUT:{}", file.display());
        }
    }
}
