use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

use xlsxpdf_memory::MemoryGovernor;
use xlsxpdf_render::RenderCallbacks;

pub type XlsxpdfDiagnosticCallback =
    Option<unsafe extern "C" fn(severity: c_int, code: u32, message: *const c_char, user_data: *mut c_void)>;

#[repr(C)]
pub struct XlsxpdfOptions {
    pub memory_limit_bytes: u64,
    pub temp_limit_bytes: u64,
    pub temp_directory: *const c_char,
    pub font_directory: *const c_char,
    pub strict_mode: c_int,
}

impl Default for XlsxpdfOptions {
    fn default() -> Self {
        Self {
            memory_limit_bytes: 256 << 20,
            temp_limit_bytes: 4 << 30,
            temp_directory: ptr::null(),
            font_directory: ptr::null(),
            strict_mode: 0,
        }
    }
}

pub struct XlsxpdfContext {
    options: XlsxpdfOptions,
    diag_callback: XlsxpdfDiagnosticCallback,
    diag_user_data: *mut c_void,
}

#[repr(C)]
pub struct XlsxpdfResult {
    pub status: c_int,
    pub page_count: u32,
    error_message: *mut c_char,
}

pub const XLSXPDF_OK: c_int = 0;
pub const XLSXPDF_ERROR_IO: c_int = 1;
pub const XLSXPDF_ERROR_FORMAT: c_int = 2;
pub const XLSXPDF_ERROR_MEMORY: c_int = 3;
pub const XLSXPDF_ERROR_INVALID_ARG: c_int = 4;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_context_create() -> *mut XlsxpdfContext {
    let ctx = Box::new(XlsxpdfContext {
        options: XlsxpdfOptions::default(),
        diag_callback: None,
        diag_user_data: ptr::null_mut(),
    });
    Box::into_raw(ctx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_context_destroy(ctx: *mut XlsxpdfContext) {
    if !ctx.is_null() {
        drop(unsafe { Box::from_raw(ctx) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_set_memory_limit(ctx: *mut XlsxpdfContext, bytes: u64) -> c_int {
    if ctx.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }
    unsafe { (*ctx).options.memory_limit_bytes = bytes };
    XLSXPDF_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_set_temp_limit(ctx: *mut XlsxpdfContext, bytes: u64) -> c_int {
    if ctx.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }
    unsafe { (*ctx).options.temp_limit_bytes = bytes };
    XLSXPDF_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_set_strict_mode(ctx: *mut XlsxpdfContext, strict: c_int) -> c_int {
    if ctx.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }
    unsafe { (*ctx).options.strict_mode = strict };
    XLSXPDF_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_set_diagnostics_callback(
    ctx: *mut XlsxpdfContext,
    callback: XlsxpdfDiagnosticCallback,
    user_data: *mut c_void,
) -> c_int {
    if ctx.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }
    let ctx = unsafe { &mut *ctx };
    ctx.diag_callback = callback;
    ctx.diag_user_data = user_data;
    XLSXPDF_OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_convert_file(
    ctx: *const XlsxpdfContext,
    input_path: *const c_char,
    output_path: *const c_char,
    result: *mut XlsxpdfResult,
) -> c_int {
    if ctx.is_null() || input_path.is_null() || output_path.is_null() || result.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }

    let ctx = unsafe { &*ctx };
    let result = unsafe { &mut *result };
    result.status = XLSXPDF_OK;
    result.page_count = 0;
    result.error_message = ptr::null_mut();

    let input = match unsafe { CStr::from_ptr(input_path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_error(result, XLSXPDF_ERROR_INVALID_ARG, "invalid input path encoding");
            return result.status;
        }
    };

    let output = match unsafe { CStr::from_ptr(output_path) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_error(result, XLSXPDF_ERROR_INVALID_ARG, "invalid output path encoding");
            return result.status;
        }
    };

    match convert_impl(ctx, input, output) {
        Ok(pages) => {
            result.status = XLSXPDF_OK;
            result.page_count = pages;
        }
        Err(e) => {
            let (code, msg) = classify_error(&e);
            set_error(result, code, &msg);
        }
    }

    result.status
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_result_status(result: *const XlsxpdfResult) -> c_int {
    if result.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }
    unsafe { (*result).status }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_result_page_count(result: *const XlsxpdfResult) -> u32 {
    if result.is_null() {
        return 0;
    }
    unsafe { (*result).page_count }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_result_error_message(result: *const XlsxpdfResult) -> *const c_char {
    if result.is_null() {
        return ptr::null();
    }
    let msg = unsafe { (*result).error_message };
    if msg.is_null() {
        ptr::null()
    } else {
        msg as *const c_char
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_result_free(result: *mut XlsxpdfResult) {
    if result.is_null() {
        return;
    }
    let r = unsafe { &mut *result };
    if !r.error_message.is_null() {
        drop(unsafe { CString::from_raw(r.error_message) });
        r.error_message = ptr::null_mut();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xlsxpdf_options_default(opts: *mut XlsxpdfOptions) -> c_int {
    if opts.is_null() {
        return XLSXPDF_ERROR_INVALID_ARG;
    }
    unsafe { *opts = XlsxpdfOptions::default() };
    XLSXPDF_OK
}

fn set_error(result: &mut XlsxpdfResult, code: c_int, message: &str) {
    result.status = code;
    if let Ok(cstr) = CString::new(message) {
        result.error_message = cstr.into_raw();
    }
}

fn classify_error(e: &xlsxpdf_core::error::Error) -> (c_int, String) {
    match e {
        xlsxpdf_core::error::Error::Io(io_err) => {
            (XLSXPDF_ERROR_IO, format!("I/O error: {io_err}"))
        }
        xlsxpdf_core::error::Error::Memory(mem_err) => {
            (XLSXPDF_ERROR_MEMORY, format!("memory limit: {mem_err}"))
        }
        xlsxpdf_core::error::Error::ResourceLimit(msg) => {
            (XLSXPDF_ERROR_MEMORY, format!("resource limit: {msg}"))
        }
        other => (XLSXPDF_ERROR_FORMAT, format!("{other}")),
    }
}

struct CapiCallbacks {
    diag_callback: XlsxpdfDiagnosticCallback,
    diag_user_data: *mut c_void,
}

unsafe impl Send for CapiCallbacks {}

impl CapiCallbacks {
    fn emit(&self, severity: c_int, code: u32, message: &str) {
        if let Some(cb) = self.diag_callback {
            if let Ok(cstr) = CString::new(message) {
                unsafe {
                    cb(severity, code, cstr.as_ptr(), self.diag_user_data);
                }
            }
        }
    }
}

impl RenderCallbacks for CapiCallbacks {
    fn on_sheet_start(&self, name: &str) {
        self.emit(0, 1000, &format!("converting sheet: {name}"));
    }

    fn on_sheet_stats(&self, _rows: usize, _cols: usize, _cells: usize, _merges: usize) {}

    fn on_pages(&self, _count: usize, _scale: f64) {}

    fn on_progress(&self, _pages_written: usize) {}

    fn on_warning(&self, message: &str) {
        self.emit(1, 2000, message);
    }
}

fn convert_impl(
    ctx: &XlsxpdfContext,
    input: &str,
    output: &str,
) -> xlsxpdf_core::error::Result<u32> {
    use xlsxpdf_opc::{OpcPackage, PackageLimits, ZipArchive};

    let governor = MemoryGovernor::new(ctx.options.memory_limit_bytes);

    let file = File::open(input)?;
    let reader = BufReader::new(file);
    let limits = PackageLimits::default();
    let mut package = OpcPackage::open(ZipArchive::new(reader, &limits)?, &limits)?;

    let out_file = File::create(output)?;
    let buf_writer = BufWriter::new(out_file);

    let callbacks = CapiCallbacks {
        diag_callback: ctx.diag_callback,
        diag_user_data: ctx.diag_user_data,
    };

    let result = xlsxpdf_render::convert(
        &mut package,
        buf_writer,
        &governor,
        &callbacks,
    )?;

    Ok(result.total_pages)
}
