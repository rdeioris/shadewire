/* C99 tests of the graph builder of the Shadewire C API: a material built call by call gives the IR of its XML
 * twin, errors name the host's source path, bad arguments and foreign handles are refused, and a builder lowers
 * more than once. Each test returns 0 on success, otherwise its failed step with a message in `report`. */

#include "shadewire.h"

#include <stdio.h>
#include <string.h>

int shadewire_ctest_builder_matches_xml(char *report, size_t report_len);
int shadewire_ctest_builder_error_names_source_path(char *report, size_t report_len);
int shadewire_ctest_builder_rejects_null_and_bad_input(char *report, size_t report_len);
int shadewire_ctest_builder_to_ir_twice(char *report, size_t report_len);

static int fail(char *report, size_t report_len, int step, const char *what, const shadewire_error *err) {
    const char *message = err != NULL ? shadewire_error_message(err, NULL) : "";
    snprintf(report, report_len, "%s%s%s", what, err != NULL ? ": " : "", message != NULL ? message : "");
    return step;
}

/* The XML twin of the material `build_twin` makes, as in shadewire-nodes' builder tests. */
static const char twin_xml[] =
    "<materialx version=\"1.39\">\n"
    "  <nodegraph name=\"NG\">\n"
    "    <input name=\"tint\" type=\"color3\" value=\"0.5, 0.25, 1\"/>\n"
    "    <input name=\"rough\" type=\"float\" value=\"0.4\"/>\n"
    "    <image name=\"img\" type=\"color3\" nodedef=\"ND_image_color3\">\n"
    "      <input name=\"file\" type=\"filename\" value=\"wood.png\" colorspace=\"srgb_texture\"/>\n"
    "      <input name=\"frameoffset\" type=\"integer\" value=\"0\"/>\n"
    "    </image>\n"
    "    <multiply name=\"mul\" type=\"color3\">\n"
    "      <input name=\"in1\" type=\"color3\" nodename=\"img\"/>\n"
    "      <input name=\"in2\" type=\"color3\" interfacename=\"tint\"/>\n"
    "    </multiply>\n"
    "    <separate3 name=\"sep\" type=\"multioutput\" nodedef=\"ND_separate3_color3\">\n"
    "      <input name=\"in\" type=\"color3\" nodename=\"mul\"/>\n"
    "    </separate3>\n"
    "    <output name=\"base_color\" type=\"color3\" nodename=\"mul\"/>\n"
    "    <output name=\"metal\" type=\"float\" nodename=\"sep\" output=\"outr\"/>\n"
    "    <output name=\"roughness\" type=\"float\" interfacename=\"rough\"/>\n"
    "  </nodegraph>\n"
    "  <standard_surface name=\"SR\" type=\"surfaceshader\" nodedef=\"ND_standard_surface_surfaceshader\">\n"
    "    <input name=\"base_color\" type=\"color3\" nodegraph=\"NG\" output=\"base_color\"/>\n"
    "    <input name=\"metalness\" type=\"float\" nodegraph=\"NG\" output=\"metal\"/>\n"
    "    <input name=\"specular_roughness\" type=\"float\" nodegraph=\"NG\" output=\"roughness\"/>\n"
    "    <input name=\"base\" type=\"float\" value=\"0.8\"/>\n"
    "    <input name=\"thin_walled\" type=\"boolean\" value=\"true\"/>\n"
    "  </standard_surface>\n"
    "  <surfacematerial name=\"M\" type=\"material\">\n"
    "    <input name=\"surfaceshader\" type=\"surfaceshader\" nodename=\"SR\"/>\n"
    "  </surfacematerial>\n"
    "</materialx>\n";

static shadewire_value floats(const float *values, size_t count) {
    shadewire_value v;
    memset(&v, 0, sizeof v);
    v.kind = SHADEWIRE_VALUE_FLOATS;
    v.floats = values;
    v.count = count;
    return v;
}

static shadewire_value integer(int32_t i) {
    shadewire_value v;
    memset(&v, 0, sizeof v);
    v.kind = SHADEWIRE_VALUE_INTEGER;
    v.integer = i;
    return v;
}

static shadewire_value boolean(bool b) {
    shadewire_value v;
    memset(&v, 0, sizeof v);
    v.kind = SHADEWIRE_VALUE_BOOLEAN;
    v.boolean = b;
    return v;
}

static shadewire_value string(const char *s) {
    shadewire_value v;
    memset(&v, 0, sizeof v);
    v.kind = SHADEWIRE_VALUE_STRING;
    v.string = s;
    return v;
}

static shadewire_source from_node(const shadewire_node *n, const char *output) {
    shadewire_source s;
    memset(&s, 0, sizeof s);
    s.kind = SHADEWIRE_SOURCE_NODE;
    s.node = n;
    s.output = output;
    return s;
}

static shadewire_source from_graph(const shadewire_nodegraph *ng, const char *output) {
    shadewire_source s;
    memset(&s, 0, sizeof s);
    s.kind = SHADEWIRE_SOURCE_NODEGRAPH;
    s.nodegraph = ng;
    s.output = output;
    return s;
}

static shadewire_source from_interface(const char *name) {
    shadewire_source s;
    memset(&s, 0, sizeof s);
    s.kind = SHADEWIRE_SOURCE_INTERFACE;
    s.interface_name = name;
    return s;
}

/* Builds the twin material into `b`, through the C API only; non-OK on the first failed call. */
static shadewire_status build_twin(shadewire_builder *b, shadewire_error **err) {
    static const float tint[3] = {0.5f, 0.25f, 1.0f};
    static const float rough = 0.4f;
    static const float base = 0.8f;
    shadewire_nodegraph *ng = NULL;
    shadewire_node *img = NULL, *mul = NULL, *sep = NULL, *sr = NULL;
    shadewire_value v;
    shadewire_source s;

#define TRY(call)                                                                                                      \
    do {                                                                                                               \
        if ((call) != SHADEWIRE_OK) {                                                                                  \
            return SHADEWIRE_ERROR;                                                                                    \
        }                                                                                                              \
    } while (0)

    TRY(shadewire_builder_add_nodegraph(b, "NG", "/M/NG", &ng, err));
    v = floats(tint, 3);
    TRY(shadewire_nodegraph_add_input(b, ng, "tint", "color3", &v, NULL, err));
    v = floats(&rough, 1);
    TRY(shadewire_nodegraph_add_input(b, ng, "rough", "float", &v, NULL, err));
    TRY(shadewire_builder_add_node(b, ng, "img", "ND_image_color3", "/M/NG/img", &img, err));
    v = string("wood.png");
    TRY(shadewire_node_set_value(b, img, "file", NULL, &v, NULL, err));
    TRY(shadewire_node_set_colorspace(b, img, "file", "srgb_texture", err));
    v = integer(0);
    TRY(shadewire_node_set_value(b, img, "frameoffset", NULL, &v, NULL, err));
    TRY(shadewire_builder_add_node_category(b, ng, "multiply", "color3", "mul", NULL, &mul, err));
    s = from_node(img, NULL);
    TRY(shadewire_node_connect(b, mul, "in1", "color3", &s, NULL, err));
    s = from_interface("tint");
    TRY(shadewire_node_connect(b, mul, "in2", "color3", &s, NULL, err));
    TRY(shadewire_builder_add_node(b, ng, "sep", "ND_separate3_color3", NULL, &sep, err));
    s = from_node(mul, NULL);
    TRY(shadewire_node_connect(b, sep, "in", NULL, &s, NULL, err));
    TRY(shadewire_nodegraph_add_output(b, ng, "base_color", "color3", &s, NULL, err));
    s = from_node(sep, "outr");
    TRY(shadewire_nodegraph_add_output(b, ng, "metal", "float", &s, NULL, err));
    s = from_interface("rough");
    TRY(shadewire_nodegraph_add_output(b, ng, "roughness", "float", &s, NULL, err));
    TRY(shadewire_builder_add_node(b, NULL, "SR", "ND_standard_surface_surfaceshader", NULL, &sr, err));
    s = from_graph(ng, "base_color");
    TRY(shadewire_node_connect(b, sr, "base_color", NULL, &s, NULL, err));
    s = from_graph(ng, "metal");
    TRY(shadewire_node_connect(b, sr, "metalness", NULL, &s, NULL, err));
    s = from_graph(ng, "roughness");
    TRY(shadewire_node_connect(b, sr, "specular_roughness", NULL, &s, NULL, err));
    v = floats(&base, 1);
    TRY(shadewire_node_set_value(b, sr, "base", NULL, &v, NULL, err));
    v = boolean(true);
    TRY(shadewire_node_set_value(b, sr, "thin_walled", NULL, &v, NULL, err));
    TRY(shadewire_builder_add_material(b, "M", sr, NULL, err));
#undef TRY
    return SHADEWIRE_OK;
}

int shadewire_ctest_builder_matches_xml(char *report, size_t report_len) {
    shadewire_builder *b = shadewire_builder_new();
    shadewire_ir *built = NULL, *parsed = NULL;
    shadewire_error *err = NULL;
    const char *built_json, *parsed_json;
    int step = 0;

    if (b == NULL) {
        step = fail(report, report_len, 1, "shadewire_builder_new returned NULL", NULL);
        goto done;
    }
    if (build_twin(b, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 2, "building the twin failed", err);
        goto done;
    }
    if (shadewire_builder_to_ir(b, "M", NULL, &built, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 3, "lowering the built material failed", err);
        goto done;
    }
    if (shadewire_mtlx_to_ir(twin_xml, strlen(twin_xml), NULL, NULL, NULL, &parsed, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 4, "lowering the XML twin failed", err);
        goto done;
    }
    built_json = shadewire_ir_json(built, NULL);
    parsed_json = shadewire_ir_json(parsed, NULL);
    if (built_json == NULL || parsed_json == NULL || strcmp(built_json, parsed_json) != 0) {
        step = fail(report, report_len, 5, "the built IR differs from its XML twin", NULL);
        goto done;
    }

done:
    shadewire_ir_free(built);
    shadewire_ir_free(parsed);
    shadewire_error_free(err);
    shadewire_builder_free(b);
    return step;
}

int shadewire_ctest_builder_error_names_source_path(char *report, size_t report_len) {
    shadewire_builder *b = shadewire_builder_new();
    shadewire_node *n = NULL;
    shadewire_error *err = NULL;
    const char *message;
    int step = 0;

    if (shadewire_builder_add_node(b, NULL, "n", "ND_no_such_nodedef", "/root/Looks/n", &n, &err) == SHADEWIRE_OK) {
        step = fail(report, report_len, 1, "an unknown nodedef was accepted", NULL);
        goto done;
    }
    if (err == NULL || n != NULL) {
        step = fail(report, report_len, 2, "no error object, or out was written", NULL);
        goto done;
    }
    message = shadewire_error_message(err, NULL);
    if (message == NULL || strstr(message, "/root/Looks/n") == NULL ||
        shadewire_error_get_kind(err) == SHADEWIRE_ERROR_INVALID_ARGUMENT) {
        step = fail(report, report_len, 3, "the error does not name the source path", err);
        goto done;
    }

done:
    shadewire_error_free(err);
    shadewire_builder_free(b);
    return step;
}

/* True when `status` is an error with an error object; frees the object. */
static bool refused(shadewire_status status, shadewire_error **err) {
    bool ok = status != SHADEWIRE_OK && *err != NULL;
    shadewire_error_free(*err);
    *err = NULL;
    return ok;
}

int shadewire_ctest_builder_rejects_null_and_bad_input(char *report, size_t report_len) {
    static const float one = 1.0f;
    shadewire_builder *b = shadewire_builder_new();
    shadewire_builder *other = shadewire_builder_new();
    shadewire_nodegraph *ng = NULL;
    shadewire_node *n = NULL, *foreign = NULL;
    shadewire_error *err = NULL;
    shadewire_value v = floats(&one, 1);
    shadewire_source s;
    shadewire_ir *ir = NULL;
    int step = 0;

    if (!refused(shadewire_builder_add_nodegraph(NULL, "NG", NULL, &ng, &err), &err) || ng != NULL) {
        step = fail(report, report_len, 1, "a NULL builder was accepted", NULL);
        goto done;
    }
    if (!refused(shadewire_builder_add_nodegraph(b, NULL, NULL, &ng, &err), &err) || ng != NULL) {
        step = fail(report, report_len, 2, "a NULL name was accepted", NULL);
        goto done;
    }
    if (!refused(shadewire_builder_add_nodegraph(b, "\xff", NULL, &ng, &err), &err) || ng != NULL) {
        step = fail(report, report_len, 3, "a name of invalid UTF-8 was accepted", NULL);
        goto done;
    }
    if (shadewire_builder_add_node_category(other, NULL, "constant", "float", "c", NULL, &foreign, &err) !=
            SHADEWIRE_OK ||
        shadewire_builder_add_node_category(b, NULL, "constant", "float", "c", NULL, &n, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 4, "adding a constant failed", err);
        goto done;
    }
    if (!refused(shadewire_node_set_value(b, foreign, "value", "float", &v, NULL, &err), &err)) {
        step = fail(report, report_len, 5, "a node of another builder was accepted", NULL);
        goto done;
    }
    s = from_node(foreign, NULL);
    if (!refused(shadewire_node_connect(b, n, "value", "float", &s, NULL, &err), &err)) {
        step = fail(report, report_len, 6, "a source node of another builder was accepted", NULL);
        goto done;
    }
    s = from_graph(NULL, "out");
    if (!refused(shadewire_node_connect(b, n, "value", "float", &s, NULL, &err), &err)) {
        step = fail(report, report_len, 7, "a NULL nodegraph source was accepted", NULL);
        goto done;
    }
    if (!refused(shadewire_node_set_value(b, n, "value", "float", NULL, NULL, &err), &err) ||
        !refused(shadewire_builder_to_ir(NULL, NULL, NULL, &ir, &err), &err) || ir != NULL) {
        step = fail(report, report_len, 8, "a NULL value or builder was accepted", NULL);
        goto done;
    }
    /* Inputs are write-once. */
    if (shadewire_node_set_value(b, n, "value", "float", &v, NULL, &err) != SHADEWIRE_OK ||
        !refused(shadewire_node_set_value(b, n, "value", "float", &v, NULL, &err), &err)) {
        step = fail(report, report_len, 9, "an input was set twice", err);
        goto done;
    }

done:
    shadewire_error_free(err);
    shadewire_builder_free(other);
    shadewire_builder_free(b);
    shadewire_builder_free(NULL);
    return step;
}

int shadewire_ctest_builder_to_ir_twice(char *report, size_t report_len) {
    shadewire_builder *b = shadewire_builder_new();
    shadewire_ir *first = NULL, *second = NULL;
    shadewire_error *err = NULL;
    int step = 0;

    if (build_twin(b, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 1, "building the twin failed", err);
        goto done;
    }
    if (shadewire_builder_to_ir(b, NULL, NULL, &first, &err) != SHADEWIRE_OK ||
        shadewire_builder_to_ir(b, "M", NULL, &second, &err) != SHADEWIRE_OK) {
        step = fail(report, report_len, 2, "lowering twice failed", err);
        goto done;
    }
    if (first == second || strcmp(shadewire_ir_json(first, NULL), shadewire_ir_json(second, NULL)) != 0) {
        step = fail(report, report_len, 3, "the two lowerings differ", NULL);
        goto done;
    }

done:
    shadewire_ir_free(first);
    shadewire_ir_free(second);
    shadewire_error_free(err);
    shadewire_builder_free(b);
    return step;
}
