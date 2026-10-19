# Architecture Contract: Memory Model and Spill Formats

**Status:** Frozen for v1
**Governs:** Memory budgeting, temporary storage, and all on-disk intermediate formats

---

## 1. Fundamental Invariant

```
Workbook size may increase:
  - processing time
  - temporary storage usage

Workbook size must NOT linearly increase:
  - resident RAM usage
```

Resident memory must be explicitly budgeted. The converter must never rely
on "there should probably be enough memory."

## 2. Memory Governor

### 2.1 Central Authority

All subsystems that allocate significant memory must request it from the
`MemoryGovernor`. No subsystem may perform large allocations based directly
on workbook-provided counts without governor approval.

### 2.2 Subsystem Budgets

Default budgets for a constrained 256 MB mode:

| Subsystem          | Budget  |
|--------------------|---------|
| ZIP/XML buffers    | 12 MB   |
| SST cache          | 24 MB   |
| Text/shaping cache | 24 MB   |
| Font runtime       | 20 MB   |
| Layout working arena | 24 MB |
| Image processing   | 32 MB   |
| PDF/page arena     | 24 MB   |
| Metadata/indexes   | 16 MB   |
| Runtime/headroom   | 70 MB   |
| **Total target**   | **~246 MB** |

When the total memory limit differs from 256 MB, budgets scale proportionally.

### 2.3 Overflow Behavior

When a subsystem exceeds its budget, it must respond in this priority order:

1. Evict cache entries (LRU or frequency-based)
2. Reduce batch/window size
3. Spill to temporary storage
4. Block until memory is released by another subsystem
5. Fail with a `RESOURCE_LIMIT` diagnostic if none of the above are possible

### 2.4 Allocation Safety

Large allocations based on workbook-provided counts are prohibited.
All size arithmetic must use checked operations:

```rust
// PROHIBITED
let bytes = width * height * channels;

// REQUIRED
let bytes = width
    .checked_mul(height)?
    .checked_mul(channels)?;
```

## 3. Temporary Storage Manager (TempStore)

### 3.1 Central Authority

One central `TempStore` manages all temporary files. No module may create
temporary files independently.

### 3.2 Responsibilities

- Unique per-conversion directory (isolated from concurrent conversions)
- File creation with managed naming
- Quota accounting (total bytes written vs. configured limit)
- Free-space validation before writes
- Cleanup on completion (success or failure)
- Crash recovery: temp directory is self-contained and deletable
- Secure file permissions (no world-readable temp files)
- Atomic output finalization (write to temp, then rename)

### 3.3 Default Quota

```
temp_limit_bytes: 4 GB (configurable)
```

### 3.4 Cleanup

The `TempStore` deletes its root directory on drop. If the process crashes,
the temp directory remains but is identifiable and safe to delete.

## 4. Shared String External Store (SST)

### 4.1 Purpose

Support millions of unique shared strings without loading them all into RAM.

### 4.2 Compilation

```
sharedStrings.xml
       │
       ▼
independent compressed blocks
       │
       ├── sst.frames    (compressed block data)
       └── sst.idx       (block directory)
```

### 4.3 Block Size

Target: 128–256 KiB uncompressed per block.

Benchmark candidates: 64 KiB, 128 KiB, 256 KiB, 512 KiB, 1 MiB.

### 4.4 Block Directory Format

Each entry in `sst.idx`:

```
field                type     description
──────────────────────────────────────────────
first_string_id      u64      first SST index in this block
file_offset          u64      byte offset into sst.frames
string_count         u32      number of strings in block
compressed_size      u32      compressed block size in bytes
raw_size             u32      uncompressed block size in bytes
flags                u32      reserved
```

The directory must be small enough to remain fully resident.
For 1M unique strings at 256 KiB blocks: ~4K entries × 32 bytes = ~128 KiB.

### 4.5 Block Payload Format

```
block_header
offset[0]        (u32, byte offset to string 0 within payload)
offset[1]
...
offset[N]
string_payload   (UTF-8 text, concatenated)
rich_text_runs   (optional, per-string run metadata)
```

Simple strings: just UTF-8 bytes.
Rich strings: UTF-8 text + run boundary offsets + run style IDs.

### 4.6 Lookup Strategy

Never read each string independently (random I/O).

Use batched block scheduling:

```
Collect a batch of 32K–64K cells
       │
       ▼
Gather referenced SST IDs
       │
       ▼
Map SST ID → block number
       │
       ▼
Sort by physical block offset
       │
       ▼
Deduplicate blocks
       │
       ▼
Sequential coalesced reads
       │
       ▼
Decompress blocks
       │
       ▼
Restore original cell order
```

### 4.7 Block Cache

Bounded LRU cache under the SST memory budget (default 24 MB).
Cache key: block number. Cache value: decompressed block bytes.

## 5. Row Geometry Spill Format

### 5.1 Files

```
sheet-NNN.rows       (row height records)
sheet-NNN.row-index  (seek index for random access)
```

### 5.2 Row Record Format

Delta-encoded for compactness:

```
field          type     description
──────────────────────────────────────
row_delta      u32      rows since previous explicit record
height         u32      row height in EMU
flags          u16      hidden, custom-height, has-merge, etc.
```

Rows at default height with no flags are implied by the gap between records.

### 5.3 Row Presence Bitmap

A full bitmap for 1,048,576 rows costs exactly 128 KiB.
This bitmap tracks which rows have explicit (non-default) height.

No heap object per row is permitted.

## 6. Resolved Drawing Spill Format

### 6.1 File

```
sheet-NNN.drawings   (resolved drawing positions)
```

### 6.2 Record Format

```
field              type     description
──────────────────────────────────────────
image_id           u32      reference to media part
x_emu              i64      resolved left edge in EMU
y_emu              i64      resolved top edge in EMU
width_emu          i64      resolved width in EMU
height_emu         i64      resolved height in EMU
rotation           i32      rotation in 60,000ths of a degree
flags              u32      reserved
```

## 7. Internal File Header Contract

All persisted intermediate files must contain:

```
field              type     description
──────────────────────────────────────────
magic              [u8; 4]  file type identifier
format_version     u16      format version number
endianness         u8       0 = little-endian
feature_flags      u8       reserved
record_count       u64      number of records
data_offset        u64      byte offset to first record
checksum           u32      CRC32 of header fields
reserved           [u8; 4]  padding to 32-byte header
```

No persisted raw pointers. All offsets are byte positions from file start.

## 8. Performance Gates

### Memory

Peak RSS must remain under `memory_limit_bytes` (default 256 MB) for all
supported stress fixtures in constrained mode.

### Streaming

Resident memory must not grow proportionally with:
- Workbook row count
- Cell count
- PDF page count

### Determinism

Same font pack + same input + same converter version = equivalent layout
across macOS, Linux, and Windows.
