# XLSX-to-PDF Converter — End-to-End Implementation Plan

## 1. Project Goal

Build a production-grade, open-source `.xlsx` → PDF conversion engine optimized for:

- very large workbooks, including 1 GB+ inputs;
- bounded memory usage on consumer hardware;
- deterministic rendering across operating systems;
- strong Excel-compatible text, row-height, column-width, image-anchor, and pagination behavior;
- streaming PDF output;
- future portability through a stable C ABI and bindings for other languages.

The v1 engine intentionally prioritizes **layout fidelity and scalability** over full Excel feature parity.

---

# 2. v1 Scope

## 2.1 In Scope

- `.xlsx`
- streaming ZIP/OPC parsing
- workbook and worksheet metadata
- shared strings
- rich shared strings
- numbers, strings, booleans, errors
- cached formula results
- styles
- fonts
- fills
- borders
- number/date/percent formatting
- explicit row heights
- explicit column widths
- AutoFit-style column measurement
- wrapped text
- shrink-to-fit
- rotated text
- hidden rows and columns
- merged cells
- embedded images
- absolute anchors
- one-cell anchors
- two-cell anchors
- print areas
- paper size
- orientation
- margins
- explicit Zoom
- FitToWidth
- FitToHeight
- manual page breaks
- repeated print-title rows
- repeated print-title columns
- page ordering
- horizontal and vertical page centering
- streaming PDF generation
- diagnostics for unsupported features
- strict memory budgeting
- temporary spill files
- deterministic font resolution

## 2.2 Deferred from v1

- formula recalculation
- full Excel formula runtime
- dependency DAG execution
- charts
- pivot charts
- pivot table rendering
- conditional formatting
- advanced shapes and SmartArt
- macros/VBA execution
- PDF/A
- legacy `.xls`
- full printer-driver emulation

Formula cells are rendered using their cached value when available.

If a formula has no cached result, the converter emits a diagnostic.

Charts are detected and reported as unsupported in v1.

---

# 3. Technology Direction

## 3.1 Core Language

Recommended:

- Rust for the main engine.

Reasons:

- no garbage collector;
- predictable ownership and memory behavior;
- safe concurrency;
- strong binary parsing ecosystem;
- excellent C interoperability;
- good WebAssembly potential;
- significantly lower memory-corruption risk when processing hostile documents.

## 3.2 External Components

Recommended components:

- HarfBuzz for text shaping;
- FreeType for font metrics and glyph access;
- DEFLATE/zlib-compatible implementation for XLSX ZIP entries;
- Zstandard for internal temporary compressed stores;
- a Unicode BiDi implementation;
- a Unicode line-breaking implementation;
- a custom narrow PDF backend rather than a heavy in-memory PDF DOM.

## 3.3 Public ABI

The core should expose a stable C ABI eventually.

Conceptually:

```c
typedef struct xlsxpdf_context xlsxpdf_context;

typedef struct {
    uint64_t memory_limit_bytes;
    uint64_t temp_limit_bytes;

    const char* temp_directory;
    const char* font_directory;

    int fidelity_mode;
    int strict_mode;
} xlsxpdf_options;

int xlsxpdf_convert_file(
    const char* input,
    const char* output,
    const xlsxpdf_options* options,
    xlsxpdf_result* result
);
```

Bindings can later be written for:

- C++
- Java
- Python
- .NET
- Node.js
- Go
- WebAssembly

Internal OOXML structures must never become part of the public API.

---

# 4. Repository Layout

```text
xlsxpdf/
│
├── Cargo.toml
├── README.md
├── LICENSE
│
├── crates/
│   ├── xlsxpdf-core/
│   ├── xlsxpdf-opc/
│   ├── xlsxpdf-xml/
│   ├── xlsxpdf-model/
│   ├── xlsxpdf-sst/
│   ├── xlsxpdf-styles/
│   ├── xlsxpdf-fonts/
│   ├── xlsxpdf-text/
│   ├── xlsxpdf-layout/
│   ├── xlsxpdf-pagination/
│   ├── xlsxpdf-drawing/
│   ├── xlsxpdf-pdf/
│   ├── xlsxpdf-memory/
│   ├── xlsxpdf-diagnostics/
│   └── xlsxpdf-capi/
│
├── apps/
│   └── xlsxpdf-cli/
│
├── tests/
│   ├── fixtures/
│   ├── golden/
│   ├── oracle/
│   ├── fuzz/
│   └── stress/
│
├── benchmarks/
│
├── docs/
│   ├── architecture.md
│   ├── memory-model.md
│   ├── typography.md
│   ├── pagination.md
│   ├── file-formats.md
│   ├── compatibility.md
│   └── diagnostics.md
│
└── tools/
    ├── workbook-generator/
    ├── oracle-runner/
    ├── pdf-diff/
    └── fixture-builder/
```

---

# 5. Fundamental Runtime Invariant

The central scalability requirement is:

```text
Workbook size may increase:
- processing time
- temporary storage usage

Workbook size must NOT linearly increase:
- resident RAM usage
```

Resident memory must be explicitly budgeted.

A converter must never rely on "there should probably be enough memory".

---

# 6. Memory Governor

Implement a central `MemoryGovernor`.

Every major subsystem receives a budget.

Example for a constrained 256 MB mode:

```text
ZIP/XML buffers              12 MB
SST cache                    24 MB
text/shaping cache           24 MB
font runtime                 20 MB
layout working arena         24 MB
image processing             32 MB
PDF/page arena               24 MB
metadata/indexes             16 MB
runtime/headroom             70 MB
---------------------------------
target                      ~246 MB
```

Subsystem behavior when exceeding its budget:

1. evict cache entries;
2. reduce batch/window size;
3. spill to temporary storage;
4. block until memory is released;
5. fail with a resource-limit diagnostic if none are possible.

Large allocations based directly on workbook-provided counts are prohibited.

---

# 7. Global Processing Pipeline

The v1 processing pipeline is frozen as:

```text
INPUT XLSX
   │
   ▼
PASS 0
Secure package discovery
   │
   ▼
PASS 0B
Global metadata compilation
   │
   ▼
PASS 1
Final column geometry
   │
   ▼
PASS 2A
Text measurement + final row geometry
   │
   ▼
PASS 2B
Drawing-anchor resolution + pagination
   │
   ▼
PASS 3
Streaming PDF generation
   │
   ▼
OUTPUT PDF
```

Critical rule:

> Each pass freezes geometry for the passes that follow.

After Pass 1:

- column widths are immutable.

After Pass 2A:

- row heights are immutable;
- text wrapping decisions are immutable.

Pass 2B must never ask typography to reflow text.

---

# 8. Pass 0 — Secure ZIP/OPC Reader

## 8.1 Purpose

Safely inspect and expose XLSX package parts without unpacking the complete archive.

## 8.2 Initial Parts

Process:

```text
[Content_Types].xml
_rels/.rels
xl/workbook.xml
xl/_rels/workbook.xml.rels
```

Discover:

- worksheets;
- styles;
- shared strings;
- themes;
- drawings;
- media;
- external links;
- workbook relationships.

## 8.3 ZIP Entry Index

Use the central directory to build a compact entry table.

Conceptually:

```rust
struct ZipEntry {
    compressed_offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    crc32: u32,
    compression_method: u16,
    flags: u16,
}
```

No archive-wide decompression.

## 8.4 Security Checks

Before parsing XML:

- maximum entry count;
- maximum expanded bytes;
- maximum individual entry size;
- maximum compression ratio;
- duplicate paths;
- path traversal;
- malformed central-directory offsets;
- CRC mismatch;
- unsupported encryption;
- nested archive policy;
- disk-space availability;
- ZIP-bomb limits.

Use checked arithmetic everywhere.

Never do:

```rust
let bytes = width * height * channels;
```

Use:

```rust
let bytes = width
    .checked_mul(height)?
    .checked_mul(channels)?;
```

---

# 9. Streaming XML Parser

DOM parsing is prohibited for large worksheet files.

Use a pull/SAX-style parser.

Conceptual events:

```rust
enum XmlEvent<'a> {
    Start(TagId),
    End(TagId),
    Attribute(AttributeId, &'a [u8]),
    Text(&'a [u8]),
}
```

The parser should expose byte slices into the current input buffer.

Avoid:

```text
XML attribute
→ String
→ substring
→ integer parser
```

Prefer:

```text
XML buffer slice
→ direct integer/reference parser
```

Disable or reject:

- DTDs;
- external entities;
- recursive entity expansion;
- pathological nesting;
- oversized attributes;
- malformed UTF sequences.

---

# 10. Cell Coordinate Encoding

Excel worksheet limits:

```text
rows    = 1,048,576 = 2^20
columns =    16,384 = 2^14
```

Therefore:

```text
row    → 20 bits
column → 14 bits
```

Local worksheet coordinate:

```rust
let key =
    ((row as u64) << 14)
    | column as u64;
```

Cross-sheet key:

```rust
let key =
    ((sheet_id as u64) << 34)
    | ((row as u64) << 14)
    | column as u64;
```

Parse references such as:

```text
A1
AA500
XFD1048576
```

directly from bytes with zero temporary string allocation.

---

# 11. Pass 0B — Metadata Compiler

Compile global workbook structures before layout.

Inputs include:

- `workbook.xml`
- `styles.xml`
- `theme.xml`
- `sharedStrings.xml`
- worksheet relationships
- drawing relationships
- defined names
- print areas
- print titles
- page setup
- media relationships

Possible temporary artifacts:

```text
workbook.meta
styles.bin
fonts.bin
sst.frames
sst.idx

sheet-000.meta
sheet-001.meta
...
```

Internal file formats must contain:

```text
magic
format version
endianness
feature flags
record counts
offsets
checksums
```

No persisted raw pointers.

---

# 12. Shared String External Store

## 12.1 Goal

Support millions of unique shared strings without loading them all into RAM.

Compile:

```text
sharedStrings.xml
       │
       ▼
independent compressed blocks
       │
       ├── sst.frames
       └── sst.idx
```

## 12.2 Block Size

Initial target:

```text
128–256 KiB uncompressed per block
```

Benchmark:

- 64 KiB
- 128 KiB
- 256 KiB
- 512 KiB
- 1 MiB

## 12.3 Directory

Conceptually:

```rust
struct SstBlockDir {
    first_string_id: u64,
    file_offset: u64,
    string_count: u32,
    compressed_size: u32,
    raw_size: u32,
    flags: u32,
}
```

This directory should be small enough to remain resident.

## 12.4 Block Format

```text
block header
offset[0]
offset[1]
...
offset[N]
string payload
rich-text run metadata
```

Simple strings stay cheap.

Rich strings store:

```text
UTF-8 text
+
run boundary offsets
+
run style IDs
```

## 12.5 Pass 2 Lookup Strategy

Never randomly read each string independently.

Use a bounded cell batch:

```text
32K–64K cells
```

Collect referenced SST IDs.

Then:

```text
SST ID
→ determine block
→ sort by physical block offset
→ deduplicate blocks
→ coalesced sequential reads
→ decompress blocks
→ restore original cell order
```

This avoids random I/O thrashing.

---

# 13. Styles Compiler

Normalize styles into canonical IDs.

Conceptual form:

```rust
struct CanonicalStyle {
    font_id: u32,
    fill_id: u32,
    border_id: u32,
    num_fmt_id: u32,
    alignment_id: u32,
    flags: u32,
}
```

Deduplicate equivalent visual styles.

Hot rendering paths should use compact IDs rather than OOXML inheritance chains.

---

# 14. Deterministic Font Registry

Host fonts alone must not determine layout.

Resolution:

```text
requested font
     ↓
exact configured font
     ↓
metric-compatible substitute
     ↓
script fallback
     ↓
final Unicode fallback
```

Persist/track:

```text
font hash
face index
units per em
ascent
descent
line gap
weight
italic
Unicode coverage
```

Both measurement and final PDF rendering must use the same resolved font identity.

---

# 15. Geometry Units

Use:

```text
worksheet/page coordinates:
    int64 EMU

typographic layout:
    fixed point
```

Recommended text metric formats:

- 26.6 fixed point; or
- 32.16 fixed point.

Do not use floating point as the authoritative workbook geometry representation.

---

# 16. Pass 1 — Column Geometry Compiler

## 16.1 Objective

Determine final immutable column widths.

Maintain resident arrays:

```rust
column_width[16384]
autofit_max[16384]
column_flags[16384]
column_style[16384]
```

This is inexpensive because Excel has only 16,384 columns.

## 16.2 Excel Logical Width

Use an authoritative Excel width representation:

```rust
type ColumnWidth256 = u32;
```

Preserve 1/256-character style logical units until final geometry is resolved.

Only then convert into physical EMU.

## 16.3 AutoFit Scan

For every AutoFit-relevant cell:

```text
cell raw value
      ↓
display formatter
      ↓
text measurement
      ↓
candidate width
      ↓
max(autofit_max[col], candidate)
```

Do not retain the cell.

State complexity is proportional to column count, not row count.

---

# 17. Display Formatting Layer

Typography operates on displayed values, not raw values.

Examples:

```text
44197
→ 01-Jan-2021

0.153
→ 15.3%

1234.5
→ $1,234.50
```

Implement:

```rust
fn format_cell(
    value: CellValue,
    format: NumberFormatId,
    locale: LocaleProfile,
) -> DisplayText
```

Cached formula results pass through the same formatting layer.

---

# 18. Typography Engine

## 18.1 Text Pipeline

```text
DisplayText
   ↓
Unicode decoding
   ↓
paragraph segmentation
   ↓
BiDi
   ↓
script segmentation
   ↓
font fallback segmentation
   ↓
line-break opportunities
   ↓
HarfBuzz shaping
   ↓
logical + ink metrics
```

HarfBuzz is the shaping component, not the complete paragraph-layout engine.

## 18.2 Fast Paths

Use tiered measurement:

```text
Tier 0
empty

Tier 1
numbers/simple ASCII

Tier 2
simple Latin/Greek/Cyrillic

Tier 3
complex shaping
```

Fast paths must produce metrics compatible with the full path.

## 18.3 Metrics

Each shaped run should produce:

```rust
struct GlyphRunMetrics {
    logical_advance: Fixed,

    ink_left: Fixed,
    ink_right: Fixed,
    ink_top: Fixed,
    ink_bottom: Fixed,

    ascent: Fixed,
    descent: Fixed,
}
```

Use:

- logical advance for wrapping;
- ink bounds for clipping safety.

## 18.4 Hinting

Layout should use unhinted/vector font metrics.

Raster hinting must not change page geometry.

---

# 19. Measurement Cache

Key:

```text
text hash
font ID
font size
weight
italic
direction
script
language
shaping features
```

Memory budget:

```text
~24 MB initial target
```

No unbounded hash map.

Use a frequency-aware admission policy for repeated strings.

---

# 20. Pass 2A — Final Row Geometry

At the start of Pass 2A:

```text
all column widths are final
```

Process rows sequentially.

For every row:

```text
read cells
  ↓
resolve styles
  ↓
format display values
  ↓
compute cell available width
  ↓
line wrapping/shrink/rotation
  ↓
cell vertical extent
  ↓
row-height resolver
  ↓
emit compact final row metric
```

After Pass 2A:

```text
all row heights are final
all wrap decisions are final
```

No later pass may modify them.

---

# 21. Row Metadata

Use:

```text
default row height
+
128 KiB row-presence bitmap
+
dense explicit override records
```

A full bitmap for 1,048,576 rows costs only:

```text
128 KiB
```

Do not create a heap object per row.

Sequential rendering should maintain a cursor into explicit overrides.

---

# 22. Row Height Resolver

Centralize all row-height logic.

Conceptually:

```rust
fn resolve_row_height(
    row: RowLayoutInput,
    metrics: &[CellVerticalMetric],
    profile: &ExcelMetricProfile,
) -> RowHeight
```

State machine:

```text
hidden?
    yes → render height 0
    no
     ↓
explicit custom height?
    yes → preserve explicit height
    no
     ↓
auto-height eligible cells
     ↓
maximum required vertical extent
     ↓
Excel-specific rounding/profile adjustment
     ↓
final row height
```

No module outside the row-height resolver may independently alter row height.

---

# 23. Wrapping Algorithm

Compute usable width:

```text
available width =
column width
- left inset
- right inset
- border effects
- indentation
```

Then:

```text
manual newline
→ mandatory break

otherwise
→ legal Unicode line-break candidates
→ shape candidate segment
→ select longest candidate that fits
```

Use an Excel-specific greedy line fitter.

---

# 24. Excel Metric Profile

Undocumented or experimentally derived constants should live in one versioned profile:

```rust
struct ExcelMetricProfile {
    cell_left_inset: Fixed,
    cell_right_inset: Fixed,
    line_leading_adjustment: Fixed,
    baseline_adjustment: Fixed,
    wrap_epsilon: Fixed,
    // ...
}
```

Do not scatter magic constants through layout code.

---

# 25. Rich Text Line Height

For line runs:

```text
normal
bold
larger font
superscript
fallback script
```

Compute:

```text
line ascent  = max(run ascents)
line descent = max(run descents)

line height =
line ascent
+ line descent
+ profile leading
```

Total wrapped height:

```text
sum(all line heights)
```

Never use generic approximations such as:

```text
font_size × 1.2
```

---

# 26. Rotation

For original text bounds:

```text
W, H
```

and rotation angle:

```text
θ
```

bounding size:

```text
Wr = |W cos θ| + |H sin θ|
Hr = |W sin θ| + |H cos θ|
```

`Hr` participates in row auto-height.

Use precomputed/fixed-point trigonometric values for supported angle ranges.

---

# 27. Shrink-to-Fit

Treat separately from wrapping.

Conceptually:

```text
natural width = W
available = A

if W <= A:
    scale = 1
else:
    scale = A / W
```

Excel-specific quantization/minimums belong in `ExcelMetricProfile`.

---

# 28. Merged Cells

For v1 Fidelity Mode:

- preserve Excel-compatible merged-cell behavior;
- do not invent automatic row expansion for merged cells unless Excel itself would do so;
- explicitly sized merged rows remain fixed.

Enhanced merged-cell AutoFit may be introduced later as a separate non-fidelity option.

Store merge rectangles compactly:

```rust
struct MergeRect {
    row0: u32,
    row1: u32,
    col0: u16,
    col1: u16,
}
```

Sort by starting row.

During sequential layout maintain only active merges.

---

# 29. Pass 2A Output

Do not persist all glyph runs.

Persist compact geometry:

```text
sheet-000.rows
sheet-000.row-index
```

Possible record:

```text
row delta
height
flags
```

Text can be reshaped during PDF rendering using the same typography engine and bounded cache.

This trades some CPU for significantly lower storage and memory.

---

# 30. Image/Drawing Metadata Compiler

Store anchors without converting them to absolute pixels.

Conceptually:

```rust
struct RelativeAnchor {
    image_id: u32,
    anchor_type: AnchorType,

    from_row: u32,
    from_col: u16,
    from_dx_emu: i64,
    from_dy_emu: i64,

    to_row: u32,
    to_col: u16,
    to_dx_emu: i64,
    to_dy_emu: i64,

    rotation: i32,
}
```

Supported anchor types:

```text
Absolute
OneCell
TwoCell
```

---

# 31. Pass 2B — Resolve Image Geometry

At this point both:

```text
column geometry
row geometry
```

are immutable.

For a marker:

```text
x = column_prefix[col] + dx
y = row_prefix[row] + dy
```

For a two-cell anchor:

```text
x0, y0 = resolved start marker
x1, y1 = resolved end marker

width  = x1 - x0
height = y1 - y0
```

This naturally handles row expansion.

Store output:

```text
resolved-drawings.bin
```

Do not decode image pixels merely to compute placement.

---

# 32. Pagination Compiler

## 32.1 Input

```text
final column widths
final row heights
print areas
paper size
orientation
margins
headers/footers
print titles
manual page breaks
Zoom
FitToWidth
FitToHeight
drawing bounds
```

## 32.2 Output

Conceptually:

```rust
struct PagePlan {
    scale: Fixed,
    horizontal_bands: Vec<ColumnBand>,
    vertical_bands: Vec<RowBand>,
    order: PageOrder,
}
```

Avoid storing heavyweight Page objects.

---

# 33. Printable Geometry

After orientation:

```text
Portrait:
W = paper width
H = paper height

Landscape:
W = paper height
H = paper width
```

Printable dimensions:

```text
PW = W - left_margin - right_margin
PH = H - top_margin - bottom_margin
```

Headers and footers consume their appropriate page regions separately.

---

# 34. Print Regions

If explicit print areas exist:

- use them.

Otherwise determine the effective printable extent from:

- nonempty cells;
- visible formatting that affects output;
- merged ranges;
- embedded drawings.

Multiple print areas should be represented as separate regions.

---

# 35. Scaling Modes

## 35.1 Explicit Zoom

```text
S = Zoom / 100
```

When Zoom is enabled:

- FitToWidth is ignored;
- FitToHeight is ignored.

## 35.2 Fit Mode

For unscaled content dimensions:

```text
CW = content width
CH = content height
```

Printable dimensions:

```text
PW
PH
```

Requested page limits:

```text
NW = pages wide
NH = pages tall
```

Candidates:

```text
SW = NW × PW / CW
SH = NH × PH / CH
```

Use only constrained dimensions.

Then:

```text
S_candidate = min(relevant constraints)
S = min(1, S_candidate)
```

Convert to a safe discrete scale.

Then simulate page count.

If the fit constraint is violated:

```text
decrement scale
recompute
```

until valid.

---

# 36. Manual Page Break Rule

Outside Fit mode:

- manual breaks participate.

Inside Fit mode:

- manual page breaks are ignored.

This rule should be encoded centrally in the pagination compiler.

---

# 37. Pagination in Worksheet Coordinates

Once scale `S` is fixed:

```text
capacity_x = printable_width / S
capacity_y = printable_height / S
```

Perform page breaking in unscaled worksheet geometry.

This reduces cumulative rounding error.

---

# 38. Horizontal Bands

Accumulate column widths until the next column would exceed `capacity_x`.

Conceptually:

```text
start column
    ↓
consume fitting columns
    ↓
emit ColumnBand
    ↓
continue
```

Hidden columns contribute zero width.

Repeated title columns reduce available capacity on later pages.

---

# 39. Vertical Bands

Accumulate final row heights until the next row would exceed `capacity_y`.

Hidden rows contribute zero render height.

Repeated title rows reduce available capacity on later pages.

---

# 40. Print Titles

Represent:

```rust
struct PrintTitles {
    rows: Option<RowRange>,
    cols: Option<ColumnRange>,
}
```

Do not physically duplicate title rows or columns in stored layout data.

During page composition reference them as repeated regions.

---

# 41. Page Grid

Do not materialize every page as a full object.

Store:

```rust
struct RowBand {
    first: u32,
    last: u32,
}

struct ColumnBand {
    first: u16,
    last: u16,
}
```

A page is:

```text
(horizontal band index, vertical band index)
```

This allows huge page grids without huge memory usage.

---

# 42. Page Ordering

Support:

```text
DownThenOver
OverThenDown
```

Page enumeration should be separate from page-break calculation.

No geometry recomputation should happen when changing enumeration order.

---

# 43. Horizontal and Vertical Centering

For scaled content width:

```text
RW = content_width × S
```

Horizontal centering offset:

```text
offset_x = (printable_width - RW) / 2
```

Analogous formula for vertical centering.

This is a page transform only.

It must never trigger text reflow.

---

# 44. Images Crossing Pages

Never split/re-encode image pixels per page.

Use:

```text
one image object
one workbook-space transform
different page clips
```

For each page:

```text
save graphics state
apply clip rectangle
apply object transform
draw same image resource
restore graphics state
```

This avoids visible seams.

---

# 45. Pass 3 — Streaming PDF Backend

## 45.1 Core Principle

Never construct the whole PDF document in memory.

Workflow:

```text
build one page display list
      ↓
serialize page stream
      ↓
compress
      ↓
write to output
      ↓
release page arena
```

Resident memory must remain essentially constant with page count.

## 45.2 Required Features

Initial backend needs:

- PDF indirect objects;
- content streams;
- Flate compression;
- fonts;
- image XObjects;
- clipping;
- transformations;
- page tree;
- catalog;
- metadata;
- xref stream/table;
- trailer.

---

# 46. Page Display List

Use a compact command representation.

Conceptually:

```rust
enum DrawCommand {
    GlyphRun(...),
    Line(...),
    Rect(...),
    FillRect(...),
    Image(...),
    BeginClip(...),
    EndClip,
}
```

Avoid one heap allocation per drawable object.

Prefer indices into page-local packed arrays.

---

# 47. PDF Font Subsetting

Track glyph usage in bounded epochs.

Example:

```text
font subset A
pages 1–128

font subset B
pages 129–256
```

Alternative closing trigger:

- glyph-count limit;
- memory threshold.

This permits streaming without embedding complete fonts.

---

# 48. Image Handling

## 48.1 JPEG

When compatible:

```text
XLSX JPEG bytes
→ validation
→ direct PDF DCTDecode stream
```

No decode/re-encode.

## 48.2 Other Images

Decode only when required.

All decoded image memory must come from the memory governor.

Enforce:

- maximum dimensions;
- maximum pixel count;
- maximum decoded footprint;
- maximum decompression work.

Prefer strip/tile decoding for very large legitimate images.

---

# 49. Diagnostics

Unsupported or degraded behavior must never be silent.

Severity types:

```text
INFO
WARNING
ERROR
UNSUPPORTED
CORRUPT_INPUT
RESOURCE_LIMIT
```

Examples:

```text
W1032:
Sheet "Revenue", cell A42:
formula has no cached result.
Formula evaluation is disabled in v1.
```

```text
W2080:
Chart object ignored because chart rendering is not available in v1.
```

Diagnostics must contain:

- sheet;
- cell/object where applicable;
- code;
- explanation;
- severity.

---

# 50. Temporary Storage Manager

Implement one central `TempStore`.

Responsibilities:

- unique per-conversion directory;
- file creation;
- quota accounting;
- free-space validation;
- cleanup;
- crash recovery strategy;
- spill file naming;
- secure permissions;
- atomic output finalization.

No module should create random temporary files independently.

---

# 51. Excel Behaviour Oracle

This is part of the architecture, not optional QA.

## 51.1 Purpose

Determine Excel behavior that is not sufficiently documented.

## 51.2 Synthetic Workbooks

Generate controlled combinations of:

- font family;
- font size;
- bold;
- italic;
- underline;
- strings of varying lengths;
- explicit newlines;
- wrap;
- shrink;
- indent;
- rotation;
- merged cells;
- column width;
- row height;
- hidden rows/columns;
- Latin text;
- Arabic;
- Hindi;
- CJK;
- emoji;
- margins;
- paper sizes;
- Zoom;
- FitToWidth;
- FitToHeight;
- print areas;
- print titles;
- manual breaks;
- image anchors.

## 51.3 Reference Process

```text
generated XLSX
    ↓
Microsoft Excel
    ↓
reference PDF
```

Candidate:

```text
same XLSX
    ↓
our converter
    ↓
candidate PDF
```

Compare:

- page count;
- page dimensions;
- cell borders;
- row positions;
- line count;
- baseline positions;
- text bounding boxes;
- image placement;
- page breaks;
- rasterized PDF differences.

Derived constants go into versioned `ExcelMetricProfile` data.

---

# 52. Fuzzing

Fuzz early.

Targets:

- ZIP package reader;
- XML tokenizer;
- relationship parser;
- cell-coordinate parser;
- shared-string parser;
- style parser;
- number format parser;
- worksheet parser;
- merged-range parser;
- drawing-anchor parser;
- page setup parser;
- PDF serializer.

Failure conditions to prevent:

- panic;
- memory corruption;
- OOM;
- integer overflow;
- infinite loop;
- uncontrolled decompression;
- path traversal;
- oversized allocation.

---

# 53. Benchmark Corpus

At minimum:

## 53.1 Parser Throughput Workbook

```text
1M rows
simple numeric cells
```

Measures:

- XML throughput;
- coordinate parsing;
- ZIP decompression.

## 53.2 Shared-String Workbook

```text
20M shared-string references
large unique SST
```

Measures:

- SST I/O;
- block cache;
- decompression.

## 53.3 Typography Workbook

```text
large wrapped-text workload
```

Measures:

- shaping;
- wrapping;
- row-height calculations.

## 53.4 Image Workbook

```text
10K embedded images
```

Measures:

- anchor resolution;
- image memory;
- PDF reuse.

## 53.5 Pagination Workbook

Extreme:

- FitToWidth;
- FitToHeight;
- repeated titles;
- manual breaks;
- multi-page images.

---

# 54. Metrics to Record

Every benchmark should report:

```text
wall time
CPU time
peak RSS
temporary disk bytes
bytes read
bytes written
cells processed/sec
rows processed/sec
pages/sec
SST block hit ratio
SST hot-cache ratio
shape-cache hit ratio
image decode bytes
PDF output bytes
```

Performance regressions must be tracked in CI eventually.

---

# 55. Performance Acceptance Gates

Initial non-negotiable gates:

## Memory

```text
peak RSS <256 MB
```

under constrained mode for supported stress fixtures.

## Streaming

Resident memory must not grow proportionally with:

- workbook row count;
- cell count;
- PDF page count.

## PDF

Memory difference between:

```text
page 100
and
page 100,000
```

must remain effectively bounded.

## Determinism

Same:

- font pack;
- input workbook;
- converter version;

must produce equivalent layout across supported OSes.

---

# 56. Architectural Rules

Add these to `CONTRIBUTING.md`.

1. No DOM parsing for worksheet XML.
2. No heap object per cell.
3. No entire worksheet stored in RAM.
4. No unbounded cache.
5. No floating point as authoritative workbook geometry.
6. No host-dependent font layout.
7. No pagination before row/column geometry is frozen.
8. No accumulated PDF page model.
9. No silently ignored unsupported features.
10. No formula recalculation in v1.
11. No chart rendering in v1.
12. No module bypasses the memory governor for large buffers.
13. No subsystem creates unmanaged temporary files.
14. Measurement and rendering must use the same font/shaping kernel.

---

# 57. Implementation Milestones

## M0 — Repository Foundation

Deliverables:

- repository structure;
- Rust workspace;
- CI;
- basic CLI;
- benchmark harness;
- formatting/linting;
- sanitizer/fuzz configuration.

Exit condition:

```text
builds cleanly on:
- macOS
- Linux
- Windows
```

---

## M1 — Secure ZIP/OPC Reader

Implement:

- central-directory parsing;
- part lookup;
- relationship loading;
- decompression limits;
- path validation;
- package diagnostics.

CLI:

```bash
xlsxpdf inspect package workbook.xlsx
```

Exit condition:

- large XLSX can be enumerated under fixed memory;
- malformed archives fail safely.

---

## M2 — Streaming Workbook/Worksheet Parser

Implement:

- workbook metadata;
- sheet discovery;
- worksheet pull parser;
- direct cell-coordinate decoder;
- numeric cells;
- booleans;
- errors;
- inline strings;
- SST references.

CLI:

```bash
xlsxpdf inspect cells workbook.xlsx
```

Output example:

```text
Sheet: Revenue

A1 string Revenue Report
B1 number 2026
A2 string Product
B2 number 1534.98

Rows scanned: ...
Cells scanned: ...
Peak RSS: ...
```

Exit condition:

- million-row worksheet streams without increasing memory with row count.

---

## M3 — Shared String External Store

Implement:

- streaming SST compiler;
- independent compressed frames;
- directory;
- rich strings;
- bounded decompression cache;
- batch block scheduling;
- temporary store integration.

Exit condition:

- millions of unique strings work under bounded RAM.

---

## M4 — Styles and Font Compiler

Implement:

- styles.xml parsing;
- canonical styles;
- number formats;
- fonts;
- font registry;
- deterministic fallback.

Exit condition:

- cells can be mapped to stable canonical visual styles.

---

## M5 — Minimal PDF Backend

Support only:

- one sheet;
- fixed rows/columns;
- simple borders/fills;
- simple text;
- one or more pages manually defined.

No AutoFit yet.

Exit condition:

```text
XLSX cells
→ geometry
→ valid PDF
```

using final architectural modules.

---

## M6 — Deterministic Typography

Implement:

- FreeType integration;
- HarfBuzz integration;
- Unicode segmentation;
- BiDi;
- line-break candidates;
- logical metrics;
- ink metrics;
- shape cache.

Exit condition:

- text rendering is deterministic;
- no clipping in controlled font fixtures.

---

## M7 — Excel Column Width + Pass 1

Implement:

- Excel logical width representation;
- maximum digit width;
- width conversion;
- explicit widths;
- AutoFit measurement;
- immutable final width arrays.

Exit condition:

- column-width oracle fixtures match defined tolerances.

---

## M8 — Wrapping + Row Height

Implement:

- wrapping;
- explicit newlines;
- shrink-to-fit;
- rotation;
- auto-height;
- custom row height;
- hidden rows;
- rich-text line metrics.

Exit condition:

- row-height oracle suite reaches the agreed compatibility threshold.

---

## M9 — Merge Handling

Implement:

- merge metadata;
- active merge stream;
- merged rendering;
- fidelity-mode merged-cell behavior.

Exit condition:

- merged fixtures produce stable geometry and no memory blowup.

---

## M10 — Image Anchor Engine

Implement:

- media discovery;
- one-cell anchors;
- two-cell anchors;
- absolute anchors;
- EMU transforms;
- rotation;
- resolved anchor spill format.

Exit condition:

- image positions match oracle fixtures.

---

## M11 — Pagination Compiler

Implement:

- paper sizes;
- orientation;
- margins;
- Zoom;
- FitToWidth;
- FitToHeight;
- print areas;
- title rows/columns;
- manual breaks;
- page centering;
- page order;
- horizontal and vertical band generation.

Exit condition:

- pagination oracle fixtures match expected page count and boundaries.

---

## M12 — Full Streaming Pipeline

Connect:

```text
Pass 0
→ Pass 0B
→ Pass 1
→ Pass 2A
→ Pass 2B
→ Pass 3
```

Exit condition:

- large mixed workbook converts under 256 MB mode.

---

## M13 — C ABI

Implement:

- context;
- options;
- result;
- diagnostics callback;
- file conversion API;
- memory/temp limits.

Exit condition:

- standalone C example converts a workbook.

---

## M14 — Hardening

Perform:

- fuzzing;
- sanitizer runs;
- malformed ZIP suite;
- malformed XML suite;
- image bomb tests;
- low-disk-space tests;
- temp cleanup tests;
- memory-governor stress tests.

Exit condition:

- no critical fuzz/sanitizer issues;
- hostile fixtures terminate predictably.

---

## M15 — v0.1 Release

Deliver:

- CLI;
- Rust library;
- C ABI;
- compatibility matrix;
- benchmark results;
- memory documentation;
- diagnostics guide;
- known limitations.

Release name:

```text
v0.1 — Layout Core
```

---

# 58. Recommended Coding Order

Do not start with HarfBuzz or the advanced PDF renderer.

The first vertical slice should be:

```text
.xlsx
 ↓
ZIP reader
 ↓
workbook.xml
 ↓
relationships
 ↓
sheet1.xml
 ↓
stream cells
 ↓
decode coordinates
 ↓
decode basic values
 ↓
print to CLI
```

Only after the streaming foundation is proven under fixed memory should layout work begin.

---

# 59. Second Vertical Slice

Build:

```text
worksheet
 ↓
styles
 ↓
fonts
 ↓
basic text measurement
 ↓
fixed row/column geometry
 ↓
minimal PDF
```

Temporarily ignore:

- AutoFit;
- wrap;
- merges;
- images;
- pagination.

Goal:

> Prove SpreadsheetML → geometry → PDF through the final architecture.

---

# 60. Third Vertical Slice

Add:

```text
Excel width kernel
+
Pass 1 AutoFit
+
wrapped text
+
final row height
```

At this point the Excel Behaviour Oracle becomes mandatory.

Do not proceed to complex pagination while row geometry remains unstable.

---

# 61. Fourth Vertical Slice

Add:

```text
paper geometry
margins
orientation
Zoom
FitToWidth/FitToHeight
print areas
manual breaks
print titles
page order
centering
```

This creates the complete print compiler.

---

# 62. Fifth Vertical Slice

Finally add:

```text
images
anchors
page-crossing clips
image-resource reuse
```

Images should be integrated only after row geometry and pagination are trustworthy.

---

# 63. v0.1 Success Criteria

v0.1 is successful when real workbooks containing:

- millions of cells;
- multiple sheets;
- shared strings;
- common formatting;
- wrapped text;
- merged cells;
- images;
- print areas;
- print titles;
- page scaling;

can be converted with:

```text
bounded memory
stable page count
deterministic layout
no text clipping
correct row geometry
correct column geometry
correct image placement
correct page splitting
streamed PDF output
```

Unsupported features must be reported clearly.

---

# 64. Architecture Documents to Freeze Before Heavy Implementation

Create and maintain these detailed specifications:

```text
docs/01-pipeline.md
docs/02-memory-and-spill-format.md
docs/03-typography-and-row-layout.md
docs/04-pagination.md
docs/05-pdf-backend.md
```

These are architecture contracts.

If implementation requires violating one, update the contract through deliberate design review rather than silently bypassing it.

---

# 65. Immediate Next Step

Start with:

```text
M0
↓
M1
↓
M2
```

The first major engineering proof is not visual fidelity.

It is:

> Can the converter safely and correctly stream extremely large XLSX worksheets under a strict memory ceiling?

Once that foundation is proven, layer the layout engine on top.

Do not build a conventional in-memory spreadsheet model and attempt to retrofit streaming later. That would undermine the central scalability goal of the entire project.
