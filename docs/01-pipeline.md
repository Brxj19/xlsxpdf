# Architecture Contract: Global Processing Pipeline

**Status:** Frozen for v1
**Governs:** All data flow from input XLSX to output PDF

---

## 1. Pipeline Stages

The v1 converter processes an XLSX file through exactly six sequential passes.
No pass may be reordered, skipped, or run concurrently with another pass.

```text
INPUT XLSX
   │
   ▼
PASS 0 ── Secure package discovery
   │
   ▼
PASS 0B ─ Global metadata compilation
   │
   ▼
PASS 1 ── Final column geometry
   │
   ▼
PASS 2A ─ Text measurement + final row geometry
   │
   ▼
PASS 2B ─ Drawing-anchor resolution + pagination
   │
   ▼
PASS 3 ── Streaming PDF generation
   │
   ▼
OUTPUT PDF
```

## 2. Geometry Freeze Rules

These rules are the central invariant of the pipeline. Violating any of them
requires a formal design review and amendment to this document.

| After pass | What is frozen                                           |
|------------|----------------------------------------------------------|
| Pass 1     | All column widths are immutable                          |
| Pass 2A    | All row heights are immutable; all wrap decisions final  |
| Pass 2B    | All page boundaries are immutable                        |

**Pass 2B must never ask the typography engine to reflow text.**
**Pass 3 must never modify page boundaries or cell geometry.**

## 3. Pass 0 — Secure Package Discovery

### Input
Raw XLSX file (a ZIP/OPC package).

### Responsibilities
1. Parse ZIP central directory into a compact entry table.
2. Validate all security invariants before reading any XML content:
   - Maximum entry count
   - Maximum expanded bytes (total and per-entry)
   - Maximum compression ratio (zip-bomb defense)
   - Duplicate path detection
   - Path traversal detection (`..` segments)
   - Malformed central-directory offset validation
   - CRC32 verification on decompression
   - Unsupported encryption rejection
3. Build the OPC relationship graph:
   - `[Content_Types].xml`
   - `_rels/.rels`
   - `xl/workbook.xml`
   - `xl/_rels/workbook.xml.rels`
4. Discover all package parts: worksheets, styles, shared strings, themes,
   drawings, media, external links.

### Output
- `ZipArchive`: seekable entry index with on-demand decompression
- `OpcPackage`: resolved relationship graph with part paths
- List of `SheetInfo` records

### Constraints
- No archive-wide decompression. Entries are read individually on demand.
- All size arithmetic uses checked operations.

## 4. Pass 0B — Global Metadata Compilation

### Input
OPC package from Pass 0.

### Responsibilities
1. Parse and compile `styles.xml` into canonical style IDs.
2. Parse `theme.xml` for color/font theme resolution.
3. Stream `sharedStrings.xml` into the SST external store
   (compressed block frames + directory index).
4. Parse worksheet metadata: merge ranges, page setup, print areas,
   print titles, defined names.
5. Parse drawing relationships for each sheet.

### Output
- `CanonicalStyle` table (font, fill, border, numfmt, alignment)
- SST block store (`sst.frames` + `sst.idx`) on temp storage
- Per-sheet metadata records (merge ranges, page setup, drawing refs)

### Constraints
- Shared strings are not held in RAM. The SST store uses bounded block
  cache with LRU eviction under the memory governor.
- Style deduplication produces compact integer IDs for hot rendering paths.

## 5. Pass 1 — Column Geometry Compiler

### Input
- Worksheet XML (streamed)
- Canonical styles
- Font metrics

### Responsibilities
1. Read explicit column widths from `<col>` elements.
2. For AutoFit columns: stream all cells, measure display text width
   using the typography engine, track per-column maximum.
3. Convert final Excel logical widths (1/256-character units) to EMU.
4. Populate immutable column arrays:
   - `column_width[16384]`
   - `column_flags[16384]` (hidden, collapsed, etc.)
   - `column_style[16384]`

### Output
Immutable column width arrays. No later pass may modify them.

### Constraints
- State complexity is O(columns), not O(rows × columns).
- Cells are not retained after measurement.
- AutoFit measurement uses the same typography engine as Pass 2A.

## 6. Pass 2A — Final Row Geometry

### Input
- Worksheet XML (second streaming pass)
- Immutable column widths from Pass 1
- Canonical styles
- SST store
- Font metrics / typography engine

### Responsibilities
For every row, sequentially:
1. Read cells and resolve styles.
2. Format display values (number → display string via numfmt).
3. Compute available cell width (column width minus insets/borders/indent).
4. Perform line wrapping, shrink-to-fit, and rotation.
5. Compute cell vertical extent from shaped text metrics.
6. Resolve row height through the centralized row-height resolver.
7. Emit compact final row metric record.

### Output
- Immutable row height records (delta-encoded, spilled to temp storage)
- Row presence bitmap (128 KiB for 1M rows)

### Constraints
- No heap object per row. Use cursor-based streaming.
- Row height resolver is the sole authority — no other module may set row height.
- Text wrap decisions are final after this pass.

## 7. Pass 2B — Drawing Resolution + Pagination

### Input
- Immutable column widths (Pass 1)
- Immutable row heights (Pass 2A)
- Drawing anchor metadata (Pass 0B)
- Page setup metadata (Pass 0B)

### Responsibilities
1. Resolve all drawing anchors to absolute EMU coordinates using
   column/row prefix sums.
2. Run the pagination compiler:
   - Compute printable geometry (paper, margins, orientation)
   - Determine scale (Zoom, FitToWidth, FitToHeight)
   - Generate horizontal column bands
   - Generate vertical row bands
   - Apply manual page breaks (only outside Fit mode)
   - Apply print areas and print titles
   - Determine page ordering (DownThenOver / OverThenDown)

### Output
- Resolved drawing positions (`resolved-drawings.bin`)
- `PagePlan`: scale factor, column bands, row bands, page order

### Constraints
- Must never trigger text reflow.
- Image pixel data is not decoded — only placement is computed.
- Page plan uses band indices, not materialized page objects.

## 8. Pass 3 — Streaming PDF Generation

### Input
- All geometry from prior passes (column widths, row heights, page plan)
- Worksheet XML (third streaming pass for cell content)
- SST store
- Typography engine
- Resolved drawing positions
- Media files from the ZIP archive

### Responsibilities
1. Iterate pages in order (per page plan).
2. For each page:
   a. Build a compact display list (draw commands).
   b. Re-shape text using the same typography engine + bounded cache.
   c. Render borders, fills, text, images with page clips.
   d. Serialize page content stream.
   e. Compress and write to output.
   f. Release page arena.

### Output
Valid, streamable PDF file.

### Constraints
- Resident memory must remain constant with respect to page count.
- Font subsets are emitted in bounded epochs (e.g., per 128 pages).
- Images that cross page boundaries use clip transforms, not pixel splitting.
- The full PDF document is never held in memory.

## 9. Cross-Cutting Rules

1. No DOM parsing for worksheet XML. Pull/SAX only.
2. No heap object per cell retained across passes.
3. No unbounded caches anywhere in the pipeline.
4. No floating-point as authoritative geometry. Use EMU (i64) and fixed-point.
5. No host-dependent font layout. Font resolution is deterministic.
6. Measurement and rendering use the same font/shaping kernel.
7. Every unsupported feature emits a diagnostic. Silent ignoring is prohibited.
8. No formula recalculation in v1. Cached values only.
9. No chart rendering in v1. Charts emit a diagnostic.
10. All large buffers go through the memory governor.
11. All temporary files go through the TempStore.
