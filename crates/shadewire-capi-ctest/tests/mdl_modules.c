/* C99 tests of the MDL modules handle (`shadewire_mdl_modules`): one handle lowers a module file many times, with and
 * without arguments, and lists its materials; an older `shadewire_mdl_modules_options` (a `struct_size` ending before
 * `path_count`) reads as no search path. Each test returns 0 on success, otherwise its failed step with a message in
 * `report`. */

#include "shadewire.h"

#include <stdio.h>
#include <string.h>

int shadewire_ctest_mdl_modules_lower_many(const char *module_path, char *report, size_t report_len);
int shadewire_ctest_mdl_modules_old_options(char *report, size_t report_len);

static int fail(char *report, size_t report_len, int step, const char *what, const shadewire_error *err) {
    const char *message = err != NULL ? shadewire_error_message(err, NULL) : "";
    snprintf(report, report_len, "%s%s%s", what, err != NULL ? ": " : "", message != NULL ? message : "");
    return step;
}

static const char *const models[] = {"surface", "surface_unlit"};

/* Lowers `module` through `modules` with `args`, writing the IR or the error. */
static shadewire_status lower(const shadewire_mdl_modules *modules, const char *module, const shadewire_mdl_arg *args,
                              size_t count, shadewire_ir **ir, shadewire_error **err) {
    shadewire_ir_options options;
    shadewire_ir_options_default(&options);
    options.models = models;
    options.model_count = 2;
    return shadewire_mdl_modules_to_ir(modules, module, NULL, &options, args, count, sizeof(shadewire_mdl_arg), ir,
                                       err);
}

/* `module_path` is a .mdl file exporting one material with a `color tint` parameter. */
int shadewire_ctest_mdl_modules_lower_many(const char *module_path, char *report, size_t report_len) {
    static const float red[3] = {0.25f, 0.1f, 0.1f};
    shadewire_mdl_modules_options options;
    shadewire_mdl_modules *modules = NULL;
    shadewire_strings *names = NULL;
    shadewire_ir *ir = NULL;
    shadewire_error *err = NULL;
    shadewire_mdl_arg tint;
    const char *json;
    int i;
    int step = 0;

    shadewire_mdl_modules_options_default(&options);
    if (options.struct_size != sizeof options || options.paths != NULL || options.path_count != 0) {
        return fail(report, report_len, 1, "the default options are not an empty search path", NULL);
    }
    if (shadewire_mdl_modules_new(&options, &modules, &err) != SHADEWIRE_OK) {
        return fail(report, report_len, 2, "shadewire_mdl_modules_new failed", err);
    }
    for (i = 0; i < 3; ++i) {
        if (lower(modules, module_path, NULL, 0, &ir, &err) != SHADEWIRE_OK) {
            step = fail(report, report_len, 3, "lowering without arguments failed", err);
            goto done;
        }
        shadewire_ir_free(ir);
        ir = NULL;
    }

    memset(&tint, 0, sizeof tint);
    tint.name = "tint";
    tint.value.kind = SHADEWIRE_VALUE_FLOATS;
    tint.value.floats = red;
    tint.value.count = 3;
    if (lower(modules, module_path, &tint, 1, &ir, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 4, "lowering with an argument failed", err);
        goto done;
    }
    json = shadewire_ir_json(ir, NULL);
    if (json == NULL || strstr(json, "\"tint\"") == NULL || strstr(json, "0.25") == NULL) {
        step = fail(report, report_len, 5, "the color argument is not the tint uniform's default", NULL);
        goto done;
    }

    if (shadewire_mdl_modules_material_names(modules, module_path, &names, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 6, "shadewire_mdl_modules_material_names failed", err);
        goto done;
    }
    if (shadewire_strings_count(names) != 1) {
        step = fail(report, report_len, 7, "the module does not list one material", NULL);
        goto done;
    }

    if (lower(modules, NULL, NULL, 0, &ir, &err) != SHADEWIRE_ERROR ||
        shadewire_error_get_kind(err) != SHADEWIRE_ERROR_INVALID_ARGUMENT) {
        step = fail(report, report_len, 8, "a NULL module was accepted", err);
    }

done:
    shadewire_strings_free(names);
    shadewire_ir_free(ir);
    shadewire_error_free(err);
    shadewire_mdl_modules_free(modules);
    return step;
}

int shadewire_ctest_mdl_modules_old_options(char *report, size_t report_len) {
    static const char *const paths[] = {NULL};
    shadewire_mdl_modules_options options;
    shadewire_mdl_modules *modules = NULL;
    shadewire_error *err = NULL;

    /* A caller that knows only `paths`: `path_count` is past its struct and reads as 0, so the NULL entry is unread. */
    shadewire_mdl_modules_options_default(&options);
    options.paths = paths;
    options.path_count = 1;
    options.struct_size = (uint32_t)offsetof(shadewire_mdl_modules_options, path_count);
    if (shadewire_mdl_modules_new(&options, &modules, &err) != SHADEWIRE_OK) {
        return fail(report, report_len, 1, "an older struct_size was refused", err);
    }
    shadewire_mdl_modules_free(modules);
    modules = NULL;

    /* The whole struct: the NULL entry is read and reported. */
    options.struct_size = sizeof options;
    if (shadewire_mdl_modules_new(&options, &modules, &err) != SHADEWIRE_ERROR || modules != NULL ||
        shadewire_error_get_kind(err) != SHADEWIRE_ERROR_INVALID_ARGUMENT) {
        shadewire_mdl_modules_free(modules);
        return fail(report, report_len, 2, "a NULL search path entry was accepted", err);
    }
    shadewire_error_free(err);
    return 0;
}
