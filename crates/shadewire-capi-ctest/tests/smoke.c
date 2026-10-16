/* C99 smoke test of the Shadewire C API: list a MaterialX file's materials, lower it to the IR and read it back as
 * JSON, compile it to WGSL and to HLSL, check a parse error, resolve includes from memory, list dependencies, write
 * an MDL module back as MDL. */

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
    shadewire_ir *ir = NULL;
    shadewire_compiled *compiled = NULL;
    shadewire_error *err = NULL;
    shadewire_target target;
    const char *code;
    int step = 0;

    shadewire_compile_options_default(&options);
    options.implicit_derivatives = true;
    target.kind = kind;
    target.version = 0;
    if (shadewire_mtlx_to_ir(text, len, path, NULL, NULL, &ir, &err) != SHADEWIRE_OK ||
        shadewire_compile(ir, target, &options, &compiled, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, first_step, "lowering or compiling failed", err);
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
    shadewire_ir_free(ir);
    shadewire_error_free(err);
    return step;
}

/* The steps of Python's `shadewire.mtlx.material_names`, `to_ir`, `Ir.optimize` and `Ir.to_dict`. */
static int names_and_ir(const char *text, size_t len, const char *path, char *report, size_t report_len) {
    shadewire_strings *names = NULL;
    shadewire_ir *ir = NULL;
    shadewire_ir *optimized = NULL;
    shadewire_document *doc = NULL;
    shadewire_error *err = NULL;
    const char *json;
    size_t json_len = 0;
    int step = 0;

    if (shadewire_mtlx_material_names(text, len, path, NULL, &names, &err) != SHADEWIRE_OK) {
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
    if (shadewire_ir_to_mtlx(ir, false, NULL, &doc, &err) != SHADEWIRE_OK ||
        shadewire_document_text(doc, NULL) == NULL) {
        step = fail(report, report_len, 13, "writing the IR back as MaterialX failed", err);
        goto done;
    }
    json = shadewire_ir_json(optimized, &json_len);
    if (json == NULL || json_len == 0 || json[0] != '{' || strcmp(shadewire_ir_source(optimized), "mtlx") != 0 ||
        shadewire_ir_structure_hash(optimized) == NULL) {
        step = fail(report, report_len, 14, "the IR does not read back", NULL);
    }

done:
    shadewire_strings_free(names);
    shadewire_ir_free(ir);
    shadewire_ir_free(optimized);
    shadewire_document_free(doc);
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

/* An MDL module lowered with the closure-tree models, written back as MDL. */
static int mdl_round_trip(char *report, size_t report_len) {
    static const char module[] = "mdl 1.7;\nimport ::df::*;\n"
                                 "export material m() = material(surface: material_surface(scattering: "
                                 "df::diffuse_reflection_bsdf()));\n";
    static const char *const models[] = {"surface", "surface_unlit"};
    shadewire_ir_options options;
    shadewire_ir *ir = NULL;
    shadewire_compiled *compiled = NULL;
    shadewire_error *err = NULL;
    shadewire_target target;
    const char *code;
    int step = 0;

    shadewire_ir_options_default(&options);
    options.models = models;
    options.model_count = 2;
    target.kind = SHADEWIRE_TARGET_MDL;
    target.version = 0;
    if (shadewire_mdl_to_ir(module, sizeof module - 1, NULL, NULL, &options, NULL, 0, &ir, &err) != SHADEWIRE_OK ||
        shadewire_compile(ir, target, NULL, &compiled, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 20, "lowering or writing MDL failed", err);
        goto done;
    }
    code = shadewire_compiled_code(compiled, NULL);
    if (code == NULL || strncmp(code, "mdl 1.7;", 8) != 0 || strcmp(shadewire_compiled_language(compiled), "mdl") != 0) {
        step = fail(report, report_len, 21, "the MDL module lacks its version line", NULL);
    }

done:
    shadewire_compiled_free(compiled);
    shadewire_ir_free(ir);
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

/* An in-memory library: the top document includes lib/a.mtlx, which includes b.mtlx (relative to lib/), which holds
 * the material. */
static const char mem_top[] = "<materialx version=\"1.39\"><xi:include href=\"lib/a.mtlx\"/></materialx>";
static const char mem_a[] = "<materialx version=\"1.39\"><xi:include href=\"b.mtlx\"/></materialx>";
static const char mem_b[] =
    "<materialx version=\"1.39\">"
    "<standard_surface name=\"SR_b\" type=\"surfaceshader\"/>"
    "<surfacematerial name=\"FromB\" type=\"material\">"
    "<input name=\"surfaceshader\" type=\"surfaceshader\" nodename=\"SR_b\"/>"
    "</surfacematerial></materialx>";

struct mem_files {
    int calls;
};

/* Joins `href` to the directory of `base` in a stack buffer, then answers from the table above. */
static void mem_resolve(void *user_data, const char *base, const char *href, shadewire_resolution *out) {
    struct mem_files *files = user_data;
    char location[256];
    const char *slash = strrchr(base, '/');
    size_t dir_len = slash != NULL ? (size_t)(slash - base) + 1 : 0;
    const char *text = NULL;
    size_t len = 0;

    files->calls++;
    if (dir_len + strlen(href) + 1 > sizeof location) {
        shadewire_resolution_fail(out, "location too long");
        return;
    }
    memcpy(location, base, dir_len);
    strcpy(location + dir_len, href);
    if (strcmp(location, "mem://lib/a.mtlx") == 0) {
        text = mem_a;
        len = sizeof mem_a - 1;
    } else if (strcmp(location, "mem://lib/b.mtlx") == 0) {
        text = mem_b;
        len = sizeof mem_b - 1;
    }
    if (text == NULL) {
        shadewire_resolution_fail(out, "not in the in-memory library");
        return;
    }
    /* Shadewire copies `location` and `text` here, so the stack buffer may go away after the return. */
    shadewire_resolution_set(out, location, text, len);
}

static int memory_includes(char *report, size_t report_len) {
    static const char missing[] = "<materialx version=\"1.39\"><xi:include href=\"nope.mtlx\"/></materialx>";
    struct mem_files files = {0};
    shadewire_mtlx_options options;
    shadewire_strings *names = NULL;
    shadewire_ir *ir = NULL;
    shadewire_error *err = NULL;
    const char *message;
    int step = 0;

    shadewire_mtlx_options_default(&options);
    options.resolve = mem_resolve;
    options.resolve_user_data = &files;
    if (shadewire_mtlx_material_names(mem_top, sizeof mem_top - 1, "mem://top.mtlx", &options, &names, &err) !=
        SHADEWIRE_OK) {
        step = fail(report, report_len, 20, "material names through the resolver failed", err);
        goto done;
    }
    if (shadewire_strings_count(names) != 1 || strcmp(shadewire_strings_get(names, 0), "FromB") != 0) {
        step = fail(report, report_len, 21, "the nested include's material is missing", NULL);
        goto done;
    }
    if (shadewire_mtlx_to_ir(mem_top, sizeof mem_top - 1, "mem://top.mtlx", NULL, &options, &ir, &err) !=
            SHADEWIRE_OK ||
        strcmp(shadewire_ir_model(ir), "standard_surface") != 0 || files.calls != 4) {
        step = fail(report, report_len, 22, "lowering through the resolver failed", err);
        goto done;
    }
    if (shadewire_mtlx_to_ir(missing, sizeof missing - 1, "mem://top.mtlx", NULL, &options, &ir, &err) !=
        SHADEWIRE_ERROR) {
        step = fail(report, report_len, 23, "a failed include was accepted", NULL);
        goto done;
    }
    message = shadewire_error_message(err, NULL);
    if (message == NULL || strstr(message, "not in the in-memory library") == NULL) {
        step = fail(report, report_len, 24, "the resolver's message is lost", err);
    }

done:
    shadewire_strings_free(names);
    shadewire_ir_free(ir);
    shadewire_error_free(err);
    return step;
}

static int dependencies(char *report, size_t report_len) {
    static const char doc[] =
        "<materialx version=\"1.39\" fileprefix=\"tex/\">"
        "<xi:include href=\"lib/a.mtlx\"/>"
        "<nodegraph name=\"g\" fileprefix=\"maps/\">"
        "<image name=\"i\" type=\"color3\"><input name=\"file\" type=\"filename\" value=\"base.png\"/></image>"
        "</nodegraph>"
        "<tiledimage name=\"t\" type=\"float\"><input name=\"file\" type=\"filename\" value=\"rough.png\"/>"
        "</tiledimage>"
        "<xi:include href=\"lib/a.mtlx\"/><xi:include href=\"lib/b.mtlx\"/>"
        "</materialx>";
    shadewire_dependencies *deps = NULL;
    shadewire_error *err = NULL;
    int step = 0;

    if (shadewire_mtlx_list_dependencies(doc, sizeof doc - 1, "mem://top.mtlx", &deps, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 30, "shadewire_mtlx_list_dependencies failed", err);
    } else if (shadewire_dependencies_include_count(deps) != 2 ||
               strcmp(shadewire_dependencies_include(deps, 0), "lib/a.mtlx") != 0 ||
               strcmp(shadewire_dependencies_include(deps, 1), "lib/b.mtlx") != 0 ||
               shadewire_dependencies_include(deps, 2) != NULL) {
        step = fail(report, report_len, 31, "the includes are wrong", NULL);
    } else if (shadewire_dependencies_file_count(deps) != 2 ||
               strcmp(shadewire_dependencies_file(deps, 0), "maps/base.png") != 0 ||
               strcmp(shadewire_dependencies_file(deps, 1), "tex/rough.png") != 0) {
        step = fail(report, report_len, 32, "the files are wrong", NULL);
    }
    shadewire_dependencies_free(deps);
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
    if (step == 0) {
        step = memory_includes(report, report_len);
    }
    if (step == 0) {
        step = dependencies(report, report_len);
    }
    if (step == 0) {
        step = mdl_round_trip(report, report_len);
    }
    if (step == 0 && shadewire_version() != header_version()) {
        step = fail(report, report_len, 9, "the header version macros differ from shadewire_version()", NULL);
    }
    return step;
}
