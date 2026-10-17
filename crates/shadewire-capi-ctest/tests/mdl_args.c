/* C99 tests of MDL arguments (`shadewire_mdl_to_ir_args`): a texture argument takes the material's texture branch,
 * a color argument is its uniform's default, and a name that is no parameter or a texture that is not a string is
 * refused. Each test returns 0 on success, otherwise its failed step with a message in `report`. */

#include "shadewire.h"

#include <stdio.h>
#include <string.h>

int shadewire_ctest_mdl_args_texture_branch(char *report, size_t report_len);
int shadewire_ctest_mdl_args_refused(char *report, size_t report_len);

static int fail(char *report, size_t report_len, int step, const char *what, const shadewire_error *err) {
    const char *message = err != NULL ? shadewire_error_message(err, NULL) : "";
    snprintf(report, report_len, "%s%s%s", what, err != NULL ? ": " : "", message != NULL ? message : "");
    return step;
}

/* The texture, when valid, replaces the color. */
static const char module[] = "mdl 1.7;\nimport ::df::*;\nimport ::tex::*;\n"
                             "export material textured(color tint = color(0.5), "
                             "uniform texture_2d tex = texture_2d()) = material(surface: material_surface("
                             "scattering: df::diffuse_reflection_bsdf(tint: tex::texture_isvalid(tex) ? "
                             "tex::lookup_color(tex, float2(0.5)) : tint)));\n";

static const char *const models[] = {"surface", "surface_unlit"};

static shadewire_mdl_arg no_arg(const char *name) {
    shadewire_mdl_arg arg;
    memset(&arg, 0, sizeof arg);
    arg.name = name;
    return arg;
}

/* Lowers `module` with `args`, elements `size` bytes apart, writing the IR or the error. */
static shadewire_status lower_sized(const shadewire_mdl_arg *args, size_t count, size_t size, shadewire_ir **ir,
                                    shadewire_error **err) {
    shadewire_ir_options options;
    shadewire_ir_options_default(&options);
    options.models = models;
    options.model_count = 2;
    return shadewire_mdl_to_ir_args(module, sizeof module - 1, NULL, NULL, &options, NULL, 0, args, count, size, ir,
                                    err);
}

/* Lowers `module` with `args`, writing the IR or the error. */
static shadewire_status lower(const shadewire_mdl_arg *args, size_t count, shadewire_ir **ir, shadewire_error **err) {
    return lower_sized(args, count, sizeof(shadewire_mdl_arg), ir, err);
}

int shadewire_ctest_mdl_args_texture_branch(char *report, size_t report_len) {
    static const float red[3] = {0.8f, 0.1f, 0.1f};
    shadewire_mdl_arg args[2];
    shadewire_ir *plain = NULL;
    shadewire_ir *textured = NULL;
    shadewire_error *err = NULL;
    const char *json;
    int step = 0;

    args[0] = no_arg("tex");
    args[0].value.kind = SHADEWIRE_VALUE_STRING;
    args[0].value.string = "textures/oak.png";
    args[0].is_texture = true;
    args[0].colorspace = "sRGB";
    args[1] = no_arg("tint");
    args[1].value.kind = SHADEWIRE_VALUE_FLOATS;
    args[1].value.floats = red;
    args[1].value.count = 3;

    if (lower(NULL, 0, &plain, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 1, "lowering without arguments failed", err);
        goto done;
    }
    json = shadewire_ir_json(plain, NULL);
    if (json == NULL || strstr(json, "\"sample\"") != NULL || strstr(json, "oak.png") != NULL) {
        step = fail(report, report_len, 2, "without arguments the material samples a texture", NULL);
        goto done;
    }
    if (lower(args, 2, &textured, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 3, "lowering with a texture argument failed", err);
        goto done;
    }
    json = shadewire_ir_json(textured, NULL);
    if (json == NULL || strstr(json, "\"sample\"") == NULL || strstr(json, "textures/oak.png") == NULL ||
        strstr(json, "srgb_texture") == NULL) {
        step = fail(report, report_len, 4, "the texture argument did not take the texture branch", NULL);
        goto done;
    }
    if (strstr(json, "\"tint\"") == NULL || strstr(json, "0.8") == NULL) {
        step = fail(report, report_len, 5, "the color argument is not the tint uniform's default", NULL);
        goto done;
    }

done:
    shadewire_ir_free(plain);
    shadewire_ir_free(textured);
    shadewire_error_free(err);
    return step;
}

int shadewire_ctest_mdl_args_refused(char *report, size_t report_len) {
    static const float one = 1.0f;
    shadewire_mdl_arg arg;
    shadewire_ir *ir = NULL;
    shadewire_error *err = NULL;
    const char *message;
    int step = 0;

    arg = no_arg("no_such_input");
    arg.value.kind = SHADEWIRE_VALUE_FLOATS;
    arg.value.floats = &one;
    arg.value.count = 1;
    if (lower(&arg, 1, &ir, &err) != SHADEWIRE_ERROR || ir != NULL) {
        step = fail(report, report_len, 1, "an unknown name was accepted", NULL);
        goto done;
    }
    message = shadewire_error_message(err, NULL);
    if (message == NULL || strstr(message, "no_such_input") == NULL) {
        step = fail(report, report_len, 2, "the error does not name the argument", err);
        goto done;
    }
    shadewire_error_free(err);
    err = NULL;

    arg = no_arg("tex");
    arg.value.kind = SHADEWIRE_VALUE_FLOATS;
    arg.value.floats = &one;
    arg.value.count = 1;
    arg.is_texture = true;
    if (lower(&arg, 1, &ir, &err) != SHADEWIRE_ERROR ||
        shadewire_error_get_kind(err) != SHADEWIRE_ERROR_INVALID_ARGUMENT) {
        step = fail(report, report_len, 3, "a texture that is not a string was accepted", err);
        goto done;
    }
    shadewire_error_free(err);
    err = NULL;

    if (shadewire_mdl_to_ir_args(module, sizeof module - 1, NULL, NULL, NULL, NULL, 0, NULL, 2,
                                 sizeof(shadewire_mdl_arg), &ir, &err) != SHADEWIRE_ERROR) {
        step = fail(report, report_len, 4, "a NULL argument list with a count was accepted", NULL);
        goto done;
    }
    shadewire_error_free(err);
    err = NULL;

    /* A caller built against an older, smaller layout. */
    arg = no_arg("tint");
    arg.value.kind = SHADEWIRE_VALUE_FLOATS;
    arg.value.floats = &one;
    arg.value.count = 1;
    if (lower_sized(&arg, 1, sizeof(shadewire_mdl_arg) - sizeof(void *), &ir, &err) != SHADEWIRE_ERROR ||
        shadewire_error_get_kind(err) != SHADEWIRE_ERROR_INVALID_ARGUMENT) {
        step = fail(report, report_len, 5, "an arg_size smaller than the struct was accepted", err);
    }

done:
    shadewire_ir_free(ir);
    shadewire_error_free(err);
    return step;
}
