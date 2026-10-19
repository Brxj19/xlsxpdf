use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, Subcommand};
use xlsxpdf_diagnostics::DiagnosticCollector;
use xlsxpdf_memory::MemoryGovernor;
use xlsxpdf_model::cell::CellValue;
use xlsxpdf_model::sheet_parser::SheetParser;
use xlsxpdf_opc::{OpcPackage, PackageLimits, ZipArchive};
use xlsxpdf_render::{self, RenderCallbacks, parse_simple_sst};

#[derive(Parser)]
#[command(name = "xlsxpdf", version, about = "XLSX to PDF converter")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect an XLSX file
    Inspect {
        #[command(subcommand)]
        what: InspectCommand,
    },
    /// Convert XLSX to PDF
    Convert {
        /// Input XLSX file
        input: PathBuf,
        /// Output PDF file
        output: PathBuf,
        /// Memory limit in MB
        #[arg(long, default_value = "256")]
        memory_mb: u64,
    },
}

#[derive(Subcommand)]
enum InspectCommand {
    /// Show package structure
    Package {
        /// Input XLSX file
        input: PathBuf,
    },
    /// Stream and display cells
    Cells {
        /// Input XLSX file
        input: PathBuf,
        /// Maximum cells to display (0 = all)
        #[arg(long, default_value = "100")]
        limit: u64,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Inspect { what } => match what {
            InspectCommand::Package { input } => cmd_inspect_package(&input),
            InspectCommand::Cells { input, limit } => cmd_inspect_cells(&input, limit),
        },
        Commands::Convert {
            input,
            output,
            memory_mb,
        } => cmd_convert(&input, &output, memory_mb),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_inspect_package(input: &PathBuf) -> xlsxpdf_core::error::Result<()> {
    let start = Instant::now();
    let file = File::open(input)?;
    let reader = BufReader::new(file);
    let limits = PackageLimits::default();
    let archive = ZipArchive::new(reader, &limits)?;

    println!("XLSX Package: {}", input.display());
    println!("Entries: {}", archive.entry_count());
    println!();

    for name in archive.entry_names() {
        let entry = archive.get_entry(name).unwrap();
        println!(
            "  {name}  (compressed: {}, uncompressed: {}, method: {})",
            entry.compressed_size, entry.uncompressed_size, entry.compression_method
        );
    }

    let package = OpcPackage::open(
        ZipArchive::new(BufReader::new(File::open(input)?), &limits)?,
        &limits,
    )?;

    println!();
    println!("Workbook: {}", package.workbook_path);
    println!("Sheets:");
    for sheet in &package.sheets {
        println!(
            "  [{}] {} -> {}",
            sheet.sheet_id, sheet.name, sheet.part_path
        );
    }
    if let Some(sst) = &package.shared_strings_path {
        println!("Shared Strings: {sst}");
    }
    if let Some(styles) = &package.styles_path {
        println!("Styles: {styles}");
    }
    if let Some(theme) = &package.theme_path {
        println!("Theme: {theme}");
    }

    println!();
    println!("Elapsed: {:.2?}", start.elapsed());

    Ok(())
}

fn cmd_inspect_cells(input: &PathBuf, limit: u64) -> xlsxpdf_core::error::Result<()> {
    let start = Instant::now();
    let file = File::open(input)?;
    let reader = BufReader::new(file);
    let limits = PackageLimits::default();
    let mut package = OpcPackage::open(ZipArchive::new(reader, &limits)?, &limits)?;

    let shared_strings = if let Some(sst_path) = package.shared_strings_path.clone() {
        let sst_data = package.read_part(&sst_path)?;
        parse_simple_sst(&sst_data)
    } else {
        Vec::new()
    };

    let sst_lookup = |id: u64| -> Option<String> { shared_strings.get(id as usize).cloned() };

    let sheets: Vec<_> = package.sheets.clone();
    let mut total_rows = 0u64;
    let mut total_cells = 0u64;

    for sheet in &sheets {
        println!("Sheet: {}", sheet.name);
        println!();

        let sheet_data = package.read_part(&sheet.part_path)?;
        let mut diagnostics = DiagnosticCollector::new();
        let mut displayed = 0u64;

        let stats = SheetParser::stream_cells(&sheet_data, &mut diagnostics, |cell| {
            if limit == 0 || displayed < limit {
                let display = cell.value.display_value(&sst_lookup);
                if !matches!(cell.value, CellValue::Empty) {
                    println!(
                        "  {} {} {}",
                        cell.reference.to_a1(),
                        cell.value.type_name(),
                        display
                    );
                    displayed += 1;
                }
            }
        })?;

        total_rows += stats.rows_scanned;
        total_cells += stats.cells_scanned;

        for diag in diagnostics.diagnostics() {
            eprintln!("  {diag}");
        }

        println!();
    }

    println!("Rows scanned: {total_rows}");
    println!("Cells scanned: {total_cells}");
    println!("Elapsed: {:.2?}", start.elapsed());

    Ok(())
}

struct CliCallbacks;

impl RenderCallbacks for CliCallbacks {
    fn on_sheet_start(&self, name: &str) {
        eprintln!("Converting sheet: {name}");
    }

    fn on_sheet_stats(&self, rows: usize, cols: usize, cells: usize, merges: usize) {
        eprintln!("  {rows} rows x {cols} cols, {cells} data cells");
        if merges > 0 {
            eprintln!("  {merges} merge regions");
        }
    }

    fn on_pages(&self, count: usize, scale: f64) {
        eprintln!("  {count} pages (scale: {:.0}%)", scale * 100.0);
    }

    fn on_progress(&self, pages_written: usize) {
        eprintln!("  Written {pages_written} pages...");
    }

    fn on_warning(&self, message: &str) {
        eprintln!("  warning: {message}");
    }

    fn on_image_bytes(&self, bytes: u64) {
        if bytes > 0 {
            eprintln!("  Image data processed: {} KB", bytes / 1024);
        }
    }
}

fn cmd_convert(
    input: &PathBuf,
    output: &PathBuf,
    memory_mb: u64,
) -> xlsxpdf_core::error::Result<()> {
    let start = Instant::now();

    let governor = MemoryGovernor::new(memory_mb << 20);
    eprintln!("Memory limit: {} MB", governor.total_limit() >> 20);

    let file = File::open(input)?;
    let reader = BufReader::new(file);
    let limits = PackageLimits::default();
    let mut package = OpcPackage::open(ZipArchive::new(reader, &limits)?, &limits)?;

    eprintln!("Parsing XLSX: {}", input.display());

    let out_file = File::create(output)?;
    let buf_writer = BufWriter::new(out_file);

    let result = xlsxpdf_render::convert(
        &mut package,
        buf_writer,
        &governor,
        &CliCallbacks,
    )?;

    eprintln!(
        "Done: {} pages written to {} ({:.2?})",
        result.total_pages,
        output.display(),
        start.elapsed()
    );

    Ok(())
}
