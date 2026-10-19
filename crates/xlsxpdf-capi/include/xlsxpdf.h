#ifndef XLSXPDF_H
#define XLSXPDF_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define XLSXPDF_OK             0
#define XLSXPDF_ERROR_IO       1
#define XLSXPDF_ERROR_FORMAT   2
#define XLSXPDF_ERROR_MEMORY   3
#define XLSXPDF_ERROR_INVALID_ARG 4

typedef struct XlsxpdfContext XlsxpdfContext;

typedef struct {
    uint64_t memory_limit_bytes;
    uint64_t temp_limit_bytes;
    const char* temp_directory;
    const char* font_directory;
    int strict_mode;
} XlsxpdfOptions;

typedef struct {
    int status;
    uint32_t page_count;
    char* error_message; /* internal — use accessor */
} XlsxpdfResult;

typedef void (*XlsxpdfDiagnosticCallback)(
    int severity, uint32_t code, const char* message, void* user_data);

XlsxpdfContext* xlsxpdf_context_create(void);
void xlsxpdf_context_destroy(XlsxpdfContext* ctx);

int xlsxpdf_set_memory_limit(XlsxpdfContext* ctx, uint64_t bytes);
int xlsxpdf_set_temp_limit(XlsxpdfContext* ctx, uint64_t bytes);
int xlsxpdf_set_strict_mode(XlsxpdfContext* ctx, int strict);
int xlsxpdf_set_diagnostics_callback(
    XlsxpdfContext* ctx, XlsxpdfDiagnosticCallback cb, void* user_data);

int xlsxpdf_convert_file(
    const XlsxpdfContext* ctx,
    const char* input_path,
    const char* output_path,
    XlsxpdfResult* result);

int xlsxpdf_result_status(const XlsxpdfResult* result);
uint32_t xlsxpdf_result_page_count(const XlsxpdfResult* result);
const char* xlsxpdf_result_error_message(const XlsxpdfResult* result);
void xlsxpdf_result_free(XlsxpdfResult* result);

int xlsxpdf_options_default(XlsxpdfOptions* opts);

#ifdef __cplusplus
}
#endif

#endif /* XLSXPDF_H */
