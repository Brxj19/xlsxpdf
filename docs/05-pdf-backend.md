# Architecture Contract: PDF Backend

**Status:** Frozen for v1
**Governs:** PDF generation, font embedding, image handling, and streaming output

---

## 1. Core Principle

**Never construct the whole PDF document in memory.**

The PDF backend is a streaming writer. Each page is built, serialized,
compressed, and written to the output file independently. After writing,
the page arena is released.

```
Build page display list
       ↓
Serialize page content stream
       ↓
Compress (Flate)
       ↓
Write to output
       ↓
Release page arena
```

Resident memory must remain essentially constant with respect to page count.

## 2. PDF Writer Architecture

### 2.1 Object Model

The writer tracks PDF indirect objects by sequential ID assignment.
Objects are written to the output file as they are created.

The writer maintains:
- Current byte offset (for xref table)
- Object offset table (Vec<u64>, one entry per object)
- Active content stream state
- Font subset state
- Image resource registry

### 2.2 Required PDF Features (v1)

| Feature              | PDF Construct              |
|----------------------|----------------------------|
| Indirect objects     | `N 0 obj ... endobj`       |
| Content streams      | Stream with /Length         |
| Flate compression    | /Filter /FlateDecode       |
| Fonts                | Type 0 / CIDFont / TrueType |
| Image XObjects       | /Subtype /Image            |
| Clipping             | `W n` operator             |
| Transformations      | `cm` operator (CTM)        |
| Page tree            | /Type /Pages               |
| Catalog              | /Type /Catalog             |
| Metadata             | /Info dictionary           |
| Cross-reference      | xref table or xref stream  |
| Trailer              | trailer dictionary         |

## 3. Page Display List

### 3.1 Command Representation

```rust
enum DrawCommand {
    SetFont(FontRef, Fixed),
    SetFillColor(Color),
    SetStrokeColor(Color),

    GlyphRun {
        x: Fixed,
        y: Fixed,
        glyphs: GlyphRunRef,    // index into page-local glyph array
    },

    MoveTo(Fixed, Fixed),
    LineTo(Fixed, Fixed),
    Rect(Fixed, Fixed, Fixed, Fixed),
    FillRect(Fixed, Fixed, Fixed, Fixed, Color),
    StrokePath,
    FillPath,

    Image {
        image_ref: ImageRef,
        x: Fixed,
        y: Fixed,
        width: Fixed,
        height: Fixed,
    },

    BeginClip(Fixed, Fixed, Fixed, Fixed),
    EndClip,

    SaveState,
    RestoreState,
    Transform(Matrix),
}
```

### 3.2 Memory Strategy

Avoid one heap allocation per drawable object.
Prefer indices into page-local packed arrays:

```
DrawCommand references:
  GlyphRunRef → index into page's glyph buffer
  FontRef     → index into document's font table
  ImageRef    → index into document's image table
```

The display list is arena-allocated and freed entirely after the page
is serialized.

## 4. Content Stream Generation

### 4.1 Operator Mapping

| Operation          | PDF Operators                              |
|--------------------|--------------------------------------------|
| Set font           | `/F1 12 Tf`                                |
| Set fill color     | `r g b rg` or `c m y k k`                 |
| Set stroke color   | `r g b RG` or `c m y k K`                 |
| Move to            | `x y m`                                    |
| Line to            | `x y l`                                    |
| Rectangle          | `x y w h re`                               |
| Fill               | `f`                                        |
| Stroke             | `S`                                        |
| Clip               | `W n`                                      |
| Show glyphs        | `[<hex>] TJ` or `<hex> Tj`                |
| Save state         | `q`                                        |
| Restore state      | `Q`                                        |
| Transform          | `a b c d e f cm`                           |
| Draw image         | `q ... /Im1 Do Q`                          |

### 4.2 Text Output

Glyph IDs are emitted as hex-encoded CID values:

```
BT
  /F1 10 Tf
  72 700 Td
  <0048 0065 006C 006C 006F> Tj
ET
```

Character-to-glyph mapping uses the same HarfBuzz shaping result
from measurement (or re-shaped in Pass 3 via the measurement cache).

### 4.3 Coordinate System

PDF uses a bottom-left origin with Y increasing upward.
Worksheet uses a top-left origin with Y increasing downward.

Each page applies a transform at the start of its content stream:

```
1 0 0 -1 0 page_height cm
```

After this, all drawing uses top-left-origin coordinates matching
worksheet geometry.

## 5. Font Subsetting

### 5.1 Epoch-Based Subsetting

Fonts are subset in bounded epochs to enable streaming:

```
Font subset A: pages 1–128
Font subset B: pages 129–256
...
```

A new epoch is triggered by:
- Page count threshold (e.g., every 128 pages)
- Glyph count threshold (e.g., 4096 unique glyphs)
- Memory threshold

### 5.2 Subset Naming

Each subset gets a unique tag prefix:

```
/AAAAAA+ArialMT    (epoch 0)
/AAAAAB+ArialMT    (epoch 1)
```

### 5.3 Font Embedding

For each font subset:
1. Collect unique glyph IDs used in the epoch.
2. Subset the font program (TrueType/OpenType).
3. Write font descriptor, CIDFont, and Type 0 font dictionaries.
4. Write CMap (Identity-H or custom ToUnicode).

### 5.4 ToUnicode CMap

Required for text extraction from the PDF. Maps glyph IDs back
to Unicode code points.

## 6. Image Handling

### 6.1 JPEG Pass-Through

When the source image is JPEG and PDF-compatible:

```
XLSX JPEG bytes → validation → direct PDF DCTDecode stream
```

No decode/re-encode. The raw JPEG bytes become the PDF image stream
with `/Filter /DCTDecode`.

Validation checks:
- Valid JPEG markers
- Supported color space (RGB, CMYK, Grayscale)
- No progressive encoding (or accept and flag)

### 6.2 PNG and Other Formats

Decode only when required. All decoded image memory comes from
the memory governor's image processing budget (default 32 MB).

Enforcement:
- Maximum image dimensions (e.g., 16384 × 16384)
- Maximum pixel count (e.g., 100M pixels)
- Maximum decoded footprint (width × height × channels, checked)
- Maximum decompression work

For very large legitimate images, prefer strip/tile decoding
to avoid loading the entire pixel buffer at once.

### 6.3 Image Reuse

If the same image appears multiple times (same media part), it is
written as a single PDF XObject and referenced by all pages that
use it. The image is not duplicated in the PDF output.

### 6.4 Image Resource Registry

```rust
struct ImageResource {
    pdf_object_id: u32,
    width: u32,
    height: u32,
    color_space: ColorSpace,
    bits_per_component: u8,
    media_path: String,
}
```

The registry maps media paths to PDF object IDs.
Once an image is written, subsequent references use the existing object.

## 7. Page Rendering Pipeline

For each page in the page plan:

```
1. Determine row/column range from band indices
2. Read cell content (streaming, from worksheet XML or spill files)
3. Resolve SST references (batched block reads)
4. Format display values
5. Shape text (using measurement cache)
6. Build display list:
   a. Fills (cell backgrounds)
   b. Borders (cell borders)
   c. Text (glyph runs positioned in cells)
   d. Images (with page clip if crossing boundary)
   e. Print titles (if applicable)
7. Apply page transforms:
   a. Scale (Zoom or Fit)
   b. Centering offset
   c. Margin offset
8. Serialize content stream
9. Compress
10. Write page object + content stream
11. Release page arena
```

## 8. Document Structure

### 8.1 Write Order

```
1. PDF header (%PDF-1.7)
2. For each page:
     - Page content stream object
     - Page dictionary object
3. Font subset objects (at epoch boundaries)
4. Image XObject objects (when first referenced)
5. Page tree object
6. Catalog object
7. Info dictionary
8. Cross-reference table
9. Trailer
10. %%EOF
```

### 8.2 Cross-Reference Table

The writer records each object's byte offset as it writes.
At the end, it writes the complete xref table.

For v1, a traditional xref table is sufficient.
xref streams may be added later for reduced file size.

## 9. Performance Constraints

### 9.1 Memory

Memory difference between rendering page 100 and page 100,000
must remain effectively bounded (within noise of the page arena size).

### 9.2 Streaming

The PDF output is written sequentially. The writer never seeks backward
to patch previously written data (except the xref offset in the trailer,
which is written last).

### 9.3 Page Arena

Each page gets a fixed-size arena (from the PDF/page arena budget,
default 24 MB). The arena is fully released after the page is written.

If a single page requires more than the arena budget (extremely dense
content), the page is rendered in bands, with each band flushed before
the next begins.

## 10. Output Finalization

1. Write all pending font subsets (final epoch).
2. Write page tree with references to all page objects.
3. Write catalog.
4. Write xref table.
5. Write trailer with xref offset.
6. Write `%%EOF`.

The output is initially written to a temporary file managed by `TempStore`.
On successful completion, it is atomically renamed to the target path.
On failure, the temporary file is cleaned up.
