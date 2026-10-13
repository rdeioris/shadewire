/* C99 smoke test of the Shadewire C API: list a MaterialX file's materials, lower it to the IR and read it back as
 * JSON, compile it to WGSL and to HLSL, check a parse error. */

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

/* Compiles the file to `kind`, checking the code names `expect`. */
static int compile_to(const char *text, size_t len, const char *path, shadewire_target_kind kind, const char *expect,
                      int first_step, char *report, size_t report_len) {
    shadewire_compile_options options;
    shadewire_compiled *compiled = NULL;
    shadewire_error *err = NULL;
    shadewire_target target;
    const char *code;
    int step = 0;

    shadewire_compile_options_default(&options);
    options.implicit_derivatives = true;
    target.kind = kind;
    target.version = 0;
    if (shadewire_compile_mtlx(text, len, path, NULL, NULL, target, &options, &compiled, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, first_step, "shadewire_compile_mtlx failed", err);
        goto done;
    }
    code = shadewire_compiled_code(compiled, NULL);
    if (code == NULL || strstr(code, expect) == NULL || shadewire_compiled_language(compiled) == NULL ||
        strcmp(shadewire_compiled_model_struct(compiled), "StandardSurface") != 0) {
        step = fail(report, report_len, first_step + 1, "the code lacks what it should have", NULL);
        goto done;
    }

done:
    shadewire_compiled_free(compiled);
    shadewire_error_free(err);
    return step;
}

/* The steps of Python's `shadewire.mtlx.material_names`, `to_ir`, `Ir.optimize` and `Ir.to_dict`. */
static int names_and_ir(const char *text, size_t len, const char *path, char *report, size_t report_len) {
    shadewire_strings *names = NULL;
    shadewire_ir *ir = NULL;
    shadewire_ir *optimized = NULL;
    shadewire_error *err = NULL;
    const char *json;
    size_t json_len = 0;
    int step = 0;

    if (shadewire_mtlx_material_names(text, len, path, &names, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 10, "shadewire_mtlx_material_names failed", err);
        goto done;
    }
    if (shadewire_strings_count(names) != 1 || shadewire_strings_get(names, 0) == NULL) {
        step = fail(report, report_len, 11, "the document should have one material", NULL);
        goto done;
    }
    if (shadewire_mtlx_to_ir(text, len, path, shadewire_strings_get(names, 0), NULL, &ir, &err) != SHADEWIRE_OK ||
        shadewire_ir_optimize(ir, &optimized, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 12, "lowering or optimizing failed", err);
        goto done;
    }
    json = shadewire_ir_json(optimized, &json_len);
    if (json == NULL || json_len == 0 || json[0] != '{' || strcmp(shadewire_ir_source(optimized), "mtlx") != 0 ||
        shadewire_ir_structure_hash(optimized) == NULL) {
        step = fail(report, report_len, 13, "the IR does not read back", NULL);
    }

done:
    shadewire_strings_free(names);
    shadewire_ir_free(ir);
    shadewire_ir_free(optimized);
    shadewire_error_free(err);
    return step;
}

static int compile_and_translate(const char *text, size_t len, const char *path, char *report, size_t report_len) {
    int step = compile_to(text, len, path, SHADEWIRE_TARGET_WGSL, "fn shadewire_eval_surface", 2, report, report_len);
    if (step == 0) {
        step = compile_to(text, len, path, SHADEWIRE_TARGET_HLSL, "shadewire_eval_surface() {", 4, report, report_len);
    }
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

static uint32_t header_version(void) {
    return ((uint32_t)SHADEWIRE_VERSION_MAJOR << 16) | ((uint32_t)SHADEWIRE_VERSION_MINOR << 8) |
           (uint32_t)SHADEWIRE_VERSION_PATCH;
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
    step = names_and_ir(text, len, mtlx_path, report, report_len);
    if (step == 0) {
        step = compile_and_translate(text, len, mtlx_path, report, report_len);
    }
    free(text);
    if (step == 0) {
        step = parse_error(report, report_len);
    }
    if (step == 0 && shadewire_version() != header_version()) {
        step = fail(report, report_len, 9, "the header version macros differ from shadewire_version()", NULL);
    }
    return step;
}
