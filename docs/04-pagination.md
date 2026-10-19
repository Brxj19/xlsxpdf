# Architecture Contract: Pagination

**Status:** Frozen for v1
**Governs:** Page breaking, scaling, print regions, title repetition, and page ordering

---

## 1. Preconditions

Pagination may only run **after** all geometry is frozen:

- All column widths are immutable (after Pass 1)
- All row heights are immutable (after Pass 2A)

Pagination must never trigger text reflow or geometry changes.

## 2. Pagination Compiler Inputs

```
final column widths        (from Pass 1)
final row heights          (from Pass 2A)
print areas                (from Pass 0B, or computed)
paper size                 (from page setup)
orientation                (portrait / landscape)
margins                    (top, bottom, left, right)
headers / footers          (height consumed from margins)
print titles               (repeated rows and/or columns)
manual page breaks         (row breaks and column breaks)
Zoom                       (explicit scale percentage)
FitToWidth                 (max pages wide)
FitToHeight                (max pages tall)
horizontal centering       (boolean)
vertical centering         (boolean)
page order                 (DownThenOver / OverThenDown)
drawing bounds             (from resolved anchors)
```

## 3. Printable Geometry

### 3.1 Paper Orientation

```
Portrait:
  W = paper_width
  H = paper_height

Landscape:
  W = paper_height
  H = paper_width
```

### 3.2 Printable Area

```
PW = W - margin_left - margin_right
PH = H - margin_top - margin_bottom
```

Headers and footers consume their height from the top/bottom margin
regions respectively. They do not reduce `PH`.

## 4. Print Regions

### 4.1 Explicit Print Areas

If the workbook defines explicit print areas, use them.
Multiple print areas produce separate regions (potentially separate
page sequences).

### 4.2 Computed Print Extent

If no explicit print area, determine the printable extent from:
- Non-empty cells
- Visible formatting that affects output
- Merged ranges
- Embedded drawings

The extent is the bounding rectangle of all visible content.

## 5. Scaling Modes

### 5.1 Explicit Zoom

```
S = Zoom / 100
```

When Zoom is set:
- FitToWidth is **ignored**
- FitToHeight is **ignored**

Zoom takes unconditional precedence.

### 5.2 Fit Mode

For unscaled content dimensions `CW` (width) and `CH` (height),
printable dimensions `PW` and `PH`, and requested page limits
`NW` (pages wide) and `NH` (pages tall):

```
Candidate scales:
  SW = (NW × PW) / CW    (only if NW is constrained)
  SH = (NH × PH) / CH    (only if NH is constrained)

S_candidate = min(all constrained candidates)
S = min(1.0, S_candidate)
```

Scale is capped at 1.0 (never enlarge).

Convert to a safe discrete scale, then simulate page count:

```
If simulated page count violates the fit constraint:
  → decrement scale
  → recompute
  → repeat until valid
```

### 5.3 No Scale

If neither Zoom nor Fit is configured:

```
S = 1.0
```

## 6. Manual Page Breaks

### 6.1 Outside Fit Mode

Manual page breaks participate in page breaking.
Row breaks force a vertical page boundary.
Column breaks force a horizontal page boundary.

### 6.2 Inside Fit Mode

**Manual page breaks are ignored.** This matches Excel behavior.

This rule is encoded centrally in the pagination compiler.

## 7. Page Breaking in Worksheet Coordinates

Once scale `S` is determined:

```
capacity_x = PW / S    (horizontal capacity in worksheet units)
capacity_y = PH / S    (vertical capacity in worksheet units)
```

Page breaking is performed in **unscaled worksheet geometry**.
This reduces cumulative rounding error from scaled coordinate arithmetic.

## 8. Horizontal Bands (Column Bands)

Accumulate column widths left-to-right until the next column would
exceed `capacity_x`:

```
start_col = first visible column in print region
accumulated = 0

for each column in print region:
    if column is hidden: skip (zero width)
    if accumulated + column_width > capacity_x:
        emit ColumnBand(start_col, prev_col)
        start_col = current_col
        accumulated = 0
    accumulated += column_width

emit final ColumnBand
```

Repeated title columns reduce available capacity on pages after the first:

```
capacity_x_subsequent = capacity_x - sum(title_column_widths)
```

### 8.1 ColumnBand Record

```rust
struct ColumnBand {
    first: u16,
    last: u16,
}
```

## 9. Vertical Bands (Row Bands)

Accumulate row heights top-to-bottom until the next row would
exceed `capacity_y`:

```
Same algorithm as horizontal bands, but with rows and capacity_y.
```

Hidden rows contribute zero render height.

Repeated title rows reduce available capacity on pages after the first:

```
capacity_y_subsequent = capacity_y - sum(title_row_heights)
```

### 9.1 RowBand Record

```rust
struct RowBand {
    first: u32,
    last: u32,
}
```

## 10. Print Titles

### 10.1 Representation

```rust
struct PrintTitles {
    rows: Option<RowRange>,    // e.g., rows 1–3
    cols: Option<ColumnRange>, // e.g., columns A–B
}
```

### 10.2 Rule

Title rows/columns are **not** physically duplicated in stored layout data.
During page composition, they are referenced as repeated regions and
rendered at the top/left of each page (after the first band in each
direction).

## 11. Page Grid

### 11.1 No Materialized Pages

Do not create a full object per page.

A page is identified by its band indices:

```
page = (horizontal_band_index, vertical_band_index)
```

This allows huge page grids (thousands of pages) without proportional
memory usage.

### 11.2 PagePlan Output

```rust
struct PagePlan {
    scale: Fixed,
    horizontal_bands: Vec<ColumnBand>,
    vertical_bands: Vec<RowBand>,
    order: PageOrder,
    print_titles: PrintTitles,
    centering_h: bool,
    centering_v: bool,
}
```

## 12. Page Ordering

Two modes:

```
DownThenOver:
  Iterate vertical bands first, then horizontal bands.
  Page sequence: (0,0), (0,1), (0,2), ..., (1,0), (1,1), ...

OverThenDown:
  Iterate horizontal bands first, then vertical bands.
  Page sequence: (0,0), (1,0), (2,0), ..., (0,1), (1,1), ...
```

Page enumeration is separate from page-break calculation.
No geometry recomputation when changing enumeration order.

## 13. Horizontal and Vertical Centering

### 13.1 Horizontal Centering

For scaled content width on a given page:

```
RW = band_content_width × S
offset_x = (PW - RW) / 2
```

### 13.2 Vertical Centering

```
RH = band_content_height × S
offset_y = (PH - RH) / 2
```

### 13.3 Rule

Centering is a **page transform only**. It must never trigger text reflow
or geometry changes. It is applied during Pass 3 rendering.

## 14. Images Crossing Pages

Images that span multiple pages are handled with clip transforms:

```
For each page that contains part of the image:
  1. Save graphics state
  2. Apply clip rectangle (page's visible region)
  3. Apply image transform (position + scale)
  4. Draw the same image resource
  5. Restore graphics state
```

**Never split or re-encode image pixels per page.**

One image object, one workbook-space transform, different page clips.
This avoids visible seams at page boundaries.

## 15. Total Page Count

```
total_pages = len(horizontal_bands) × len(vertical_bands)
```

For multi-sheet workbooks, each sheet produces its own page plan.
Total PDF pages = sum of all sheet page plans.
