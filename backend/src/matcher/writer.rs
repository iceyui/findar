//! Result workbook writer. Column layout and cell text formatting mirror the
//! legacy pandas/openpyxl output, including the `_safe_excel_text` formula-
//! injection guard and `Rp{:,}` thousand separators.

use std::path::{Path, PathBuf};

use chrono::Local;
use rust_xlsxwriter::{Workbook, XlsxError};
use uuid::Uuid;

pub struct ResultRow {
    pub target: String,
    pub customer: String,
    pub invoice_count: usize,
    pub invoices: String,
    pub dates: String,
    pub amounts: String,
    pub total: i64,
}

/// Inserts thousands separators like Python's f"{value:,}".
pub fn commas(value: i64) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (offset, ch) in digits.chars().enumerate() {
        if offset > 0 && (digits.len() - offset) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

/// Port of `_safe_excel_text`: neutralise spreadsheet formula injection.
pub fn safe_excel_text(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_string()
    }
}

pub fn build_output_path(output_dir: &Path) -> PathBuf {
    let timestamp = Local::now().format("%d%m%Y_%H%M%S");
    output_dir.join(format!(
        "hasil_kombinasi_invoice_{timestamp}_{}.xlsx",
        Uuid::new_v4().simple()
    ))
}

const HEADERS: [&str; 7] = [
    "Target",
    "Pelanggan",
    "Jumlah Invoice",
    "Invoice",
    "Tanggal",
    "Nilai",
    "Total",
];

pub fn write_results(path: &Path, rows: &[ResultRow]) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();

    sheet.write_row(0, 0, HEADERS)?;

    for (index, row) in rows.iter().enumerate() {
        let sheet_row = (index + 1) as u32;
        sheet.write(sheet_row, 0, safe_excel_text(&row.target))?;
        sheet.write(sheet_row, 1, safe_excel_text(&row.customer))?;
        sheet.write(sheet_row, 2, row.invoice_count as u32)?;
        sheet.write(sheet_row, 3, safe_excel_text(&row.invoices))?;
        sheet.write(sheet_row, 4, safe_excel_text(&row.dates))?;
        sheet.write(sheet_row, 5, safe_excel_text(&row.amounts))?;
        sheet.write(sheet_row, 6, format!("Rp{}", commas(row.total)))?;
    }

    workbook.save(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_formatting_matches_python() {
        assert_eq!(commas(1234567), "1,234,567");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1000), "1,000");
        assert_eq!(commas(100), "100");
    }

    #[test]
    fn safe_text_prefixes_formula_chars() {
        assert_eq!(safe_excel_text("=cmd"), "'=cmd");
        assert_eq!(safe_excel_text("-5"), "'-5");
        assert_eq!(safe_excel_text("INV-1"), "INV-1");
    }
}
