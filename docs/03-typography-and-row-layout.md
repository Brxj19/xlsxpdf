# Architecture Contract: Typography and Row Layout

**Status:** Frozen for v1
**Governs:** Text measurement, shaping, wrapping, row-height resolution, and all
geometry derived from text metrics

---

## 1. Coordinate Systems

### 1.1 Worksheet / Page Coordinates

```
int64 EMU (English Metric Units)
1 inch = 914,400 EMU
1 cm   = 360,000 EMU
1 pt   = 12,700 EMU
```

EMU is the authoritative coordinate type for all workbook geometry.
Floating-point must never be the authoritative representation.

### 1.2 Typographic Metrics

```
26.6 fixed point (i32)
  — or —
32.16 fixed point (i64, if 26.6 proves insufficient)
```

Compatible with FreeType's native metric format.

### 1.3 Excel Logical Column Width

```
ColumnWidth256 = u32  (1/256-character units)
```

Preserved until final geometry resolution, then converted to EMU once.

## 2. Display Formatting Layer

Typography operates on **displayed values**, never raw cell values.

```
raw value        →  display formatter  →  display text
──────────────────────────────────────────────────────
44197            →  date numfmt        →  "01-Jan-2021"
0.153            →  percent numfmt     →  "15.3%"
1234.5           →  currency numfmt    →  "$1,234.50"
```

Interface:

```rust
fn format_cell(
    value: CellValue,
    format: NumberFormatId,
    locale: LocaleProfile,
) -> DisplayText
```

Cached formula results pass through the same formatting layer.

## 3. Text Shaping Pipeline

### 3.1 Full Pipeline

```
DisplayText
   ↓
Unicode decoding
   ↓
Paragraph segmentation (U+000A, U+000D, U+2029)
   ↓
BiDi analysis (UAX #9)
   ↓
Script segmentation (UAX #24)
   ↓
Font fallback segmentation
   ↓
Line-break opportunity detection (UAX #14)
   ↓
HarfBuzz shaping (per run)
   ↓
Logical + ink metrics
```

HarfBuzz is the shaping component. It is **not** the paragraph-layout engine.
Paragraph layout, wrapping, and line breaking are implemented by xlsxpdf.

### 3.2 Tiered Fast Paths

Use tiered measurement to avoid full shaping for simple content:

| Tier | Content                        | Strategy                    |
|------|--------------------------------|-----------------------------|
| 0    | Empty cell                     | Zero metrics                |
| 1    | Numbers / simple ASCII         | Font metrics table lookup   |
| 2    | Simple Latin / Greek / Cyrillic | Single-run HarfBuzz shape  |
| 3    | Complex scripts (Arabic, Devanagari, CJK, emoji) | Full pipeline |

**Fast paths must produce metrics bit-identical to the full path for the
same input.** A fast path that disagrees with the full path is a bug.

### 3.3 Glyph Run Metrics

Each shaped run produces:

```rust
struct GlyphRunMetrics {
    logical_advance: Fixed,   // for wrapping decisions

    ink_left: Fixed,          // ink bounding box
    ink_right: Fixed,
    ink_top: Fixed,
    ink_bottom: Fixed,

    ascent: Fixed,            // typographic ascent
    descent: Fixed,           // typographic descent
}
```

Use logical advance for wrapping. Use ink bounds for clipping safety.

### 3.4 Hinting

Layout uses **unhinted / vector** font metrics only.
Raster hinting must never change page geometry.

## 4. Font Resolution

### 4.1 Deterministic Resolution Chain

```
requested font name
     ↓
exact configured font (font directory / embedded)
     ↓
metric-compatible substitute
     ↓
script-appropriate fallback
     ↓
final Unicode fallback (last resort)
```

Host fonts alone must not determine layout. The converter ships or
references a known font set. Layout produced on macOS must match Linux.

### 4.2 Font Identity Record

```
font_hash          [u8; 32]   (SHA-256 of font file)
face_index         u32
units_per_em       u16
ascent             i16
descent            i16
line_gap           i16
weight             u16
italic             bool
unicode_coverage   bitset
```

Measurement and PDF rendering must use the **same resolved font identity**.
A font substitution in rendering that differs from measurement is a bug.

## 5. Measurement Cache

### 5.1 Cache Key

```
text_hash          u64
font_id            u32
font_size_26_6     i32
weight             u16
italic             u8
direction          u8    (LTR / RTL)
script             u32
language           u32
shaping_features   u32
```

### 5.2 Budget

Default: 24 MB under the memory governor's text/shaping budget.

No unbounded hash map. Use a bounded concurrent map with LRU or
frequency-aware admission (admit repeated strings preferentially).

### 5.3 Cache Coherence

The cache is valid only within a single converter run.
It must never be persisted across conversions.

## 6. Wrapping Algorithm

### 6.1 Available Width Computation

```
available_width =
    column_width
    - cell_left_inset
    - cell_right_inset
    - border_effects
    - indentation
```

Insets and border effects come from `ExcelMetricProfile`.

### 6.2 Break Strategy

```
Manual newline (U+000A, U+000D)
  → mandatory break

Otherwise:
  → enumerate legal Unicode line-break candidates (UAX #14)
  → shape candidate segment
  → select longest candidate that fits within available_width
```

This is an **Excel-specific greedy** line fitter, not a Knuth-Plass
optimal breaker. Excel uses a greedy algorithm; we match it for fidelity.

### 6.3 Wrap Result

The wrapping pass produces a list of line records per cell:

```
struct CellLineRecord {
    line_ascent: Fixed,
    line_descent: Fixed,
    line_advance: Fixed,
    glyph_count: u16,
}
```

## 7. Shrink-to-Fit

Shrink-to-fit is separate from wrapping. They never combine.

```
natural_width = W   (measured text advance)
available     = A   (cell available width)

if W <= A:
    scale = 1.0
else:
    scale = A / W
```

Excel-specific quantization and minimum scale live in `ExcelMetricProfile`.

Shrink-to-fit renders single-line; wrapping is disabled.

## 8. Rotation

For original text bounds `(W, H)` and rotation angle `θ`:

```
Wr = |W cos θ| + |H sin θ|
Hr = |W sin θ| + |H cos θ|
```

`Hr` participates in row auto-height.

Use precomputed fixed-point trigonometric values for supported angle
ranges (0°, 45°, 90°, 135°, 180°, 255°, 270°, 315°, and the special
"stacked" vertical mode).

## 9. Rich Text Line Height

For a line containing multiple runs with varying fonts/sizes:

```
line_ascent  = max(run.ascent  for each run on this line)
line_descent = max(run.descent for each run on this line)

line_height = line_ascent + line_descent + profile.line_leading
```

Total wrapped cell height:

```
cell_height = sum(line_height for each line)
```

**Never use generic approximations** like `font_size × 1.2`.
Always compute from actual font metrics.

## 10. Row Height Resolver

### 10.1 Single Authority

The row-height resolver is the **sole module** that determines final row height.
No other module may independently alter row height.

### 10.2 Resolution State Machine

```
Is row hidden?
  YES → render height = 0, done
  NO  ↓

Has explicit custom height?
  YES → use explicit height, done
  NO  ↓

Compute auto-height:
  → collect CellVerticalMetric for each auto-height-eligible cell
  → take maximum required vertical extent
  → apply Excel-specific rounding / profile adjustment
  → final row height
```

### 10.3 Interface

```rust
fn resolve_row_height(
    row: RowLayoutInput,
    metrics: &[CellVerticalMetric],
    profile: &ExcelMetricProfile,
) -> RowHeight
```

## 11. Merged Cells

### 11.1 v1 Fidelity Mode

- Preserve Excel-compatible merged-cell behavior.
- Do NOT invent automatic row expansion for merged cells unless Excel does.
- Explicitly sized merged rows remain fixed.

### 11.2 Storage

```rust
struct MergeRect {
    row0: u32,
    row1: u32,
    col0: u16,
    col1: u16,
}
```

Sorted by starting row. During sequential layout, maintain only active
merges (those overlapping the current row window).

## 12. Excel Metric Profile

All undocumented or experimentally derived constants live in one
versioned profile structure:

```rust
struct ExcelMetricProfile {
    cell_left_inset: Fixed,
    cell_right_inset: Fixed,
    line_leading_adjustment: Fixed,
    baseline_adjustment: Fixed,
    wrap_epsilon: Fixed,
    shrink_min_scale: Fixed,
    shrink_quantization: Fixed,
    default_row_height: Emu,
    default_column_width_256: ColumnWidth256,
    // ...
}
```

**Magic constants must not be scattered through layout code.**
All such values must be read from this profile.

Profile constants are derived from the Excel Behaviour Oracle (Section 51
of the implementation plan) and versioned. Changes require a design review.

## 13. Pass 2A Output Contract

Pass 2A emits compact row geometry. It does **not** persist all glyph runs.

Text is re-shaped during Pass 3 (PDF generation) using the same typography
engine and bounded measurement cache. This trades CPU for significantly
lower intermediate storage and memory.

Persisted artifacts:

```
sheet-NNN.rows        (row height records, delta-encoded)
sheet-NNN.row-index   (seek index)
```

Format specified in `02-memory-and-spill-format.md`, Section 5.
