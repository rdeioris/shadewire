/* C99 smoke test of the Shadewire C API: compile a MaterialX file, translate it to HLSL, check a parse error. */

#include "shadewire.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int shadewire_smoke(const char *mtlx_path, char *report, size_t report_len);

static int fail(char *report, size_t report_len, int step, const char *what, const shadewire_error *err) {
    const char *message = err != NULL ? shadewire_error_message(err, NULL) : "";
    snprintf(report, report_len, "%s%s%s", what, err != NULL ? ": " : "", message != NULL ? message : "");
    return step;
}

static char *read_file(const char *path, size_t *len) {
    FILE *file = fopen(path, "rb");
    char *text = NULL;
    long size = 0;
    if (file == NULL) {
        return NULL;
    }
    if (fseek(file, 0, SEEK_END) == 0 && (size = ftell(file)) >= 0 && fseek(file, 0, SEEK_SET) == 0) {
        text = malloc((size_t)size + 1);
        if (text != NULL && fread(text, 1, (size_t)size, file) != (size_t)size) {
            free(text);
            text = NULL;
        }
    }
    fclose(file);
    if (text != NULL) {
        text[size] = '\0';
        *len = (size_t)size;
    }
    return text;
}

static int compile_and_translate(const char *text, size_t len, const char *path, char *report, size_t report_len) {
    shadewire_compile_options options;
    shadewire_compiled *compiled = NULL;
    shadewire_blob *hlsl = NULL;
    shadewire_error *err = NULL;
    shadewire_target target;
    const char *wgsl;
    const char *code;
    int step = 0;

    shadewire_compile_options_default(&options);
    if (shadewire_compile_mtlx(text, len, path, NULL, NULL, &options, &compiled, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 2, "shadewire_compile_mtlx failed", err);
        goto done;
    }
    wgsl = shadewire_compiled_wgsl(compiled, NULL);
    if (wgsl == NULL || strstr(wgsl, "shadewire_eval_surface") == NULL) {
        step = fail(report, report_len, 3, "the WGSL lacks shadewire_eval_surface", NULL);
        goto done;
    }
    target.kind = SHADEWIRE_TARGET_HLSL;
    target.version = 60;
    if (shadewire_translate(compiled, NULL, target, &hlsl, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 4, "shadewire_translate to HLSL 6.0 failed", err);
        goto done;
    }
    code = (const char *)shadewire_blob_data(hlsl, NULL);
    if (!shadewire_blob_is_text(hlsl) || code == NULL || strstr(code, "shadewire_eval_surface") == NULL) {
        step = fail(report, report_len, 5, "the HLSL lacks shadewire_eval_surface", NULL);
        goto done;
    }

done:
    shadewire_blob_free(hlsl);
    shadewire_compiled_free(compiled);
    shadewire_error_free(err);
    return step;
}

static int parse_error(char *report, size_t report_len) {
    static const char truncated[] = "<materialx";
    shadewire_ir *ir = NULL;
    shadewire_error *err = NULL;
    size_t message_len = 0;
    const char *message;
    int step = 0;

    if (shadewire_mtlx_to_ir(truncated, sizeof truncated - 1, NULL, NULL, NULL, &ir, &err) != SHADEWIRE_ERROR) {
        step = fail(report, report_len, 6, "truncated XML was accepted", NULL);
    } else if (ir != NULL || shadewire_error_get_kind(err) != SHADEWIRE_ERROR_PARSE) {
        step = fail(report, report_len, 7, "truncated XML is not a parse error", err);
    } else {
        message = shadewire_error_message(err, &message_len);
        if (message == NULL || message_len == 0 || strlen(message) != message_len) {
            step = fail(report, report_len, 8, "the parse error has no message", NULL);
        }
    }
    shadewire_ir_free(ir);
    shadewire_error_free(err);
    return step;
}

int shadewire_smoke(const char *mtlx_path, char *report, size_t report_len) {
    size_t len = 0;
    char *text = read_file(mtlx_path, &len);
    int step;

    if (report_len > 0) {
        report[0] = '\0';
    }
    if (text == NULL) {
        return fail(report, report_len, 1, "cannot read the MaterialX file", NULL);
    }
    step = compile_and_translate(text, len, mtlx_path, report, report_len);
    free(text);
    if (step != 0) {
        return step;
    }
    return parse_error(report, report_len);
}
