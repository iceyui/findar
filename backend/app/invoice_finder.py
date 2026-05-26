import os
import uuid
from datetime import datetime
from itertools import combinations

import pandas as pd


def _safe_excel_text(value):
    text = "" if value is None else str(value)
    if text.startswith(("=", "+", "-", "@", "\t", "\r")):
        return f"'{text}"
    return text


def _load_invoice_dataframe(file_path):
    # Read Excel with dynamic header row containing "Nama Pelanggan"
    try:
        df = pd.read_excel(file_path, header=None)
    except Exception as exc:
        raise ValueError(
            "Gagal membaca file Excel. Pastikan file .xlsx valid dan tidak rusak."
        ) from exc

    header_row = None
    for i, row in df.iterrows():
        if "Nama Pelanggan" in str(row.values):
            header_row = i
            break

    if header_row is None:
        raise ValueError(
            "Header tidak ditemukan. Pastikan ada kolom 'Nama Pelanggan'."
        )

    df = pd.read_excel(file_path, header=header_row)
    df.columns = [col.strip() if isinstance(col, str) else col for col in df.columns]

    # Map important columns
    col_map = {}
    for col in df.columns:
        if "Nama Pelanggan" in str(col):
            col_map["Nama Pelanggan"] = col
        elif "No. Faktur" in str(col):
            col_map["No. Faktur"] = col
        elif "Tgl. Faktur" in str(col):
            col_map["Tgl. Faktur"] = col
        elif "Total" in str(col):
            col_map["Total"] = col

    if len(col_map) < 4:
        raise ValueError(
            "Kolom penting tidak lengkap. Wajib ada: "
            "'Nama Pelanggan', 'No. Faktur', 'Tgl. Faktur', 'Total'."
        )

    df = df[
        [
            col_map["Nama Pelanggan"],
            col_map["No. Faktur"],
            col_map["Tgl. Faktur"],
            col_map["Total"],
        ]
    ]

    # Clean Total column
    df[col_map["Total"]] = pd.to_numeric(
        df[col_map["Total"]]
        .astype(str)
        .str.replace(",", "")
        .str.replace(r"\..*", "", regex=True),
        errors="coerce",
    )

    df[col_map["Tgl. Faktur"]] = pd.to_datetime(
        df[col_map["Tgl. Faktur"]], errors="coerce"
    )

    df = df.dropna()
    df = df[df[col_map["Total"]] > 0]

    if df.empty:
        raise ValueError(
            "Data kosong setelah dibersihkan. Pastikan kolom Total berisi angka "
            "dan tanggal faktur valid."
        )

    return df, col_map


def _find_results(df, col_map, target_nilai, toleransi, max_invoices):
    results = []
    for nama_pelanggan, group in df.groupby(col_map["Nama Pelanggan"], sort=False):
        invoices = group.to_dict(orient="records")

        for r in range(1, min(max_invoices + 1, len(invoices) + 1)):
            for combo in combinations(invoices, r):
                total = sum(inv[col_map["Total"]] for inv in combo)
                if abs(total - target_nilai) <= toleransi:
                    results.append(
                        {
                            "Pelanggan": _safe_excel_text(nama_pelanggan),
                            "Jumlah Invoice": r,
                            "Invoice": ", ".join(
                                _safe_excel_text(inv[col_map["No. Faktur"]])
                                for inv in combo
                            ),
                            "Tanggal": ", ".join(
                                inv[col_map["Tgl. Faktur"]].strftime("%d/%m/%Y")
                                for inv in combo
                            ),
                            "Nilai": ", ".join(
                                f"Rp{int(inv[col_map['Total']]):,}" for inv in combo
                            ),
                            "Total": f"Rp{int(total):,}",
                        }
                    )
    return results


def find_invoice_combinations(
    file_path, target_nilai, toleransi=10000, max_invoices=5, output_dir=None
):
    df, col_map = _load_invoice_dataframe(file_path)

    results = _find_results(df, col_map, target_nilai, toleransi, max_invoices)

    if not results:
        return None, 0

    timestamp = datetime.now().strftime("%d%m%Y_%H%M%S")
    output_file = f"hasil_kombinasi_invoice_{timestamp}_{uuid.uuid4().hex}.xlsx"
    if output_dir:
        output_file = os.path.join(output_dir, output_file)

    pd.DataFrame(results).to_excel(output_file, index=False)
    return output_file, len(results)


def find_invoice_combinations_for_targets(
    file_path, targets, toleransi=10000, max_invoices=5, output_dir=None
):
    df, col_map = _load_invoice_dataframe(file_path)

    all_results = []
    for target_nilai in targets:
        target_results = _find_results(
            df, col_map, target_nilai, toleransi, max_invoices
        )
        for row in target_results:
            row["Target"] = f"Rp{int(target_nilai):,}"
        all_results.extend(target_results)

    if not all_results:
        return None, 0

    timestamp = datetime.now().strftime("%d%m%Y_%H%M%S")
    output_file = f"hasil_kombinasi_invoice_{timestamp}_{uuid.uuid4().hex}.xlsx"
    if output_dir:
        output_file = os.path.join(output_dir, output_file)

    columns = [
        "Target",
        "Pelanggan",
        "Jumlah Invoice",
        "Invoice",
        "Tanggal",
        "Nilai",
        "Total",
    ]
    result_df = pd.DataFrame(all_results)[columns]
    for column in ["Target", "Pelanggan", "Invoice", "Tanggal", "Nilai", "Total"]:
        result_df[column] = result_df[column].map(_safe_excel_text)
    result_df.to_excel(output_file, index=False)
    return output_file, len(all_results)
