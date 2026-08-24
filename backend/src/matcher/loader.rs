//! Excel loading with exact behavioural parity to the legacy pandas pipeline
//! (`backend-py/app/invoice_finder.py`), quirks included by design:
//!
//! - header row = first row whose cell text contains "Nama Pelanggan"
//! - column mapping via substring match, first match wins per key
//! - Total cleaning replicates `astype(str) -> remove "," -> cut at first "."`
//!   (so string cells like "Rp1.234" intentionally become unparsable)
//! - rows missing any required field, or with Total <= 0, are dropped

use std::collections::HashMap;
use std::path::Path;

use calamine::{Data, Range, Reader, Xlsx};
use chrono::{Datelike, NaiveDate, NaiveDateTime};

use crate::matcher::engine::InvoiceRow;

#[derive(Debug)]
pub enum LoaderError {
    /// "Gagal membaca file Excel. Pastikan file .xlsx valid dan tidak rusak."
    ReadFailed,
    /// "Header tidak ditemukan. Pastikan ada kolom 'Nama Pelanggan'."
    HeaderNotFound,
    /// "Kolom penting tidak lengkap. Wajib ada: ..."
    ColumnsIncomplete,
    /// "Data kosong setelah dibersihkan. Pastikan kolom Total berisi angka ..."
    EmptyAfterClean,
}

pub struct GroupedData {
    /// Customer groups in first-appearance order (`groupby(sort=False)` parity).
    pub groups: Vec<(String, Vec<InvoiceRow>)>,
}

const HEADER_NEEDLES: [&str; 4] = ["Nama Pelanggan", "No. Faktur", "Tgl. Faktur", "Total"];

fn data_display(data: &Data) -> String {
    match data {
        Data::String(s) => s.clone(),
        other => format!("{other}"),
    }
}

/// Replicates Python's `str(float)` closely enough for the decimal-cut step:
/// whole floats keep their trailing ".0" (e.g. `1234567.0`), NaN/inf render
/// like Python ("NaN"/"inf") and fail parsing downstream.
fn py_float_repr(f: f64) -> String {
    if !f.is_finite() {
        return format!("{f}");
    }
    if f == f.trunc() && f.abs() < 1e16 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

/// Exact port of the pandas cleaning chain:
/// `astype(str) -> replace(",", "") -> replace(r"\..*", "") -> to_numeric(coerce)`
fn total_from_cell(data: &Data) -> Option<i64> {
    let raw = match data {
        Data::Float(f) => py_float_repr(*f),
        Data::Int(i) => i.to_string(),
        Data::String(s) => s.clone(),
        _ => return None,
    };

    let no_commas = raw.replace(',', "");
    let cut = match no_commas.find('.') {
        Some(dot) => &no_commas[..dot],
        None => &no_commas[..],
    };

    cut.trim().parse::<i64>().ok()
}

fn parse_date_str(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    // pandas default is month-first for ambiguous slash/dash dates.
    const FORMATS: [&str; 7] = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y",
        "%m-%d-%Y",
    ];
    for fmt in FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
            return Some(date);
        }
        if let Ok(datetime) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(datetime.date());
        }
    }
    None
}

fn date_from_cell(data: &Data) -> Option<NaiveDate> {
    match data {
        Data::DateTime(dt) => dt.as_datetime().map(|d: NaiveDateTime| d.date()),
        Data::DateTimeIso(s) => parse_date_str(s),
        Data::String(s) => parse_date_str(s),
        _ => None,
    }
}

pub fn format_date_label(date: NaiveDate) -> String {
    format!("{:02}/{:02}/{}", date.day(), date.month(), date.year())
}

struct ColumnMap {
    customer: usize,
    invoice_no: usize,
    invoice_date: usize,
    total: usize,
}

fn find_header_row(range: &Range<Data>) -> Option<usize> {
    range.rows().enumerate().find_map(|(index, row)| {
        row.iter()
            .any(|cell| data_display(cell).contains("Nama Pelanggan"))
            .then_some(index)
    })
}

fn map_columns(header_row: &[Data]) -> Option<ColumnMap> {
    let mut mapped: HashMap<&str, usize> = HashMap::new();

    'columns: for (index, cell) in header_row.iter().enumerate() {
        let name = match cell {
            Data::String(s) => s.trim().to_string(),
            other => data_display(other),
        };
        for needle in HEADER_NEEDLES {
            if mapped.contains_key(needle) {
                continue;
            }
            if name.contains(needle) {
                mapped.insert(needle, index);
                continue 'columns;
            }
        }
    }

    Some(ColumnMap {
        customer: *mapped.get("Nama Pelanggan")?,
        invoice_no: *mapped.get("No. Faktur")?,
        invoice_date: *mapped.get("Tgl. Faktur")?,
        total: *mapped.get("Total")?,
    })
}

pub fn load_grouped(path: &Path) -> Result<GroupedData, LoaderError> {
    let mut workbook: Xlsx<_> =
        calamine::open_workbook(path).map_err(|_| LoaderError::ReadFailed)?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or(LoaderError::ReadFailed)?
        .map_err(|_| LoaderError::ReadFailed)?;

    let header_index = find_header_row(&range).ok_or(LoaderError::HeaderNotFound)?;
    let columns = {
        let header_row: Vec<Data> = range.rows().nth(header_index).unwrap().to_vec();
        map_columns(&header_row).ok_or(LoaderError::ColumnsIncomplete)?
    };

    let mut order: Vec<String> = Vec::new();
    let mut lookup: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<(String, Vec<InvoiceRow>)> = Vec::new();

    for row in range.rows().skip(header_index + 1) {
        let cell = |col: usize| -> &Data {
            row.get(col).unwrap_or(&Data::Empty)
        };

        let customer_raw = data_display(cell(columns.customer));
        let customer = customer_raw.trim().to_string();
        if customer.is_empty() {
            continue;
        }

        let invoice_no_raw = data_display(cell(columns.invoice_no));
        let invoice_no = invoice_no_raw.trim().to_string();
        if invoice_no.is_empty() || invoice_no == "nan" {
            continue;
        }

        let Some(invoice_date) = date_from_cell(cell(columns.invoice_date)) else {
            continue;
        };

        let Some(amount) = total_from_cell(cell(columns.total)) else {
            continue;
        };
        if amount <= 0 {
            continue;
        }

        let next_index = groups.len();
        let group_index = *lookup.entry(customer.clone()).or_insert(next_index);
        if group_index == next_index {
            order.push(customer.clone());
            groups.push((customer.clone(), Vec::new()));
        }

        groups[group_index].1.push(InvoiceRow {
            invoice_no,
            date_label: format_date_label(invoice_date),
            amount,
        });
    }

    if groups.iter().all(|(_, rows)| rows.is_empty()) {
        return Err(LoaderError::EmptyAfterClean);
    }

    Ok(GroupedData { groups })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_repr_matches_python_for_common_cases() {
        assert_eq!(py_float_repr(1234567.0), "1234567.0");
        assert_eq!(py_float_repr(1234.5), "1234.5");
        assert_eq!(py_float_repr(0.25), "0.25");
        assert_eq!(py_float_repr(f64::NAN), "NaN");
    }

    #[test]
    fn total_parsing_replicates_pandas_quirks() {
        assert_eq!(total_from_cell(&Data::Float(1234567.0)), Some(1234567));
        assert_eq!(total_from_cell(&Data::Int(500)), Some(500));
        assert_eq!(total_from_cell(&Data::String("1,234".into())), Some(1234));
        // Quirk kept by design: everything after the FIRST dot is discarded.
        assert_eq!(total_from_cell(&Data::String("1.234.567".into())), Some(1));
        assert_eq!(total_from_cell(&Data::String("Rp1.234".into())), None);
        assert_eq!(total_from_cell(&Data::String("abc".into())), None);
        assert_eq!(total_from_cell(&Data::Bool(true)), None);
        assert_eq!(total_from_cell(&Data::Empty), None);
    }

    #[test]
    fn date_formats_month_first_like_pandas_default() {
        assert_eq!(
            parse_date_str("2024-01-05"),
            NaiveDate::from_ymd_opt(2024, 1, 5)
        );
        assert_eq!(
            parse_date_str("01/02/2024"),
            NaiveDate::from_ymd_opt(2024, 1, 2) // month-first
        );
        assert_eq!(parse_date_str("not a date"), None);
    }

    #[test]
    fn excel_datetime_iso_cells_are_read_directly() {
        let date = date_from_cell(&Data::DateTimeIso("2024-01-05T08:30:00".into()));
        assert_eq!(date, NaiveDate::from_ymd_opt(2024, 1, 5));
    }

    #[test]
    fn date_label_format_matches_python_strftime() {
        assert_eq!(
            format_date_label(NaiveDate::from_ymd_opt(2024, 3, 7).unwrap()),
            "07/03/2024"
        );
    }
}
