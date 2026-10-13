# Loads materials every way the extension offers, draws them on spheres with the real renderer, saves a
# screenshot and quits; exits 1 when a check fails. Arguments after "--": <vendored .mtlx> <screenshot .png>
# <host A url> <host B url> <cache dir> <auth header value for A> (run.py serves A and B).
extends SceneTree

const FRAMES := 30

var failures: Array[String] = []
var frame := 0
var screenshot := ""
var sw: Shadewire
var results := {}
var remote_done := false


func check(ok: bool, what: String) -> void:
	if ok:
		print("OK   ", what)
	else:
		failures.append(what)
		printerr("FAIL ", what)


func sphere(x: float, material: Material) -> void:
	var mesh := MeshInstance3D.new()
	mesh.mesh = SphereMesh.new()
	mesh.position = Vector3(x, 0, 0)
	mesh.material_override = material
	root.add_child(mesh)


func textured(material: Variant, count := 1) -> bool:
	if not material is ShaderMaterial:
		return false
	for i in count:
		if not material.get_shader_parameter("shadewire_t%d" % i) is Texture2D:
			return false
	return true


# Level k of a nodegraph chain adds two instances of level k - 1: about 2^levels nodes.
func doubling(levels: int) -> String:
	var text := """<materialx version="1.39">
		<nodedef name="ND_l0_float" node="l0"><input name="in" type="float" value="0"/><output name="out" type="float"/></nodedef>
		<nodegraph name="NG_l0_float" nodedef="ND_l0_float">
		<add name="a" type="float"><input name="in1" type="float" interfacename="in"/><input name="in2" type="float" value="1"/></add>
		<output name="out" type="float" nodename="a"/></nodegraph>"""
	for k in range(1, levels + 1):
		text += """<nodedef name="ND_l{k}_float" node="l{k}"><input name="in" type="float" value="0"/><output name="out" type="float"/></nodedef>
			<nodegraph name="NG_l{k}_float" nodedef="ND_l{k}_float">
			<l{p} name="x" type="float"><input name="in" type="float" interfacename="in"/></l{p}>
			<l{p} name="y" type="float"><input name="in" type="float" nodename="x"/></l{p}>
			<add name="a" type="float"><input name="in1" type="float" nodename="x"/><input name="in2" type="float" nodename="y"/></add>
			<output name="out" type="float" nodename="a"/></nodegraph>""".format({"k": k, "p": k - 1})
	text += """<l{n} name="top" type="float"/>
		<standard_surface name="SR" type="surfaceshader"><input name="base" type="float" nodename="top"/></standard_surface>
		<surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR"/></surfacematerial>
		</materialx>""".format({"n": levels})
	return text


const FRACTAL := """<materialx version="1.39">
	<nodegraph name="NG">
		<input name="octaves" type="integer" value="1000000"/>
		<fractal3d name="f" type="color3"><input name="octaves" type="integer" interfacename="octaves"/></fractal3d>
		<output name="out" type="color3" nodename="f"/>
	</nodegraph>
	<standard_surface name="SR" type="surfaceshader"><input name="base_color" type="color3" nodegraph="NG" output="out"/></standard_surface>
	<surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR"/></surfacematerial>
	</materialx>"""


func write(path: String, text: String) -> void:
	DirAccess.make_dir_recursive_absolute(path.get_base_dir())
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string(text)
	file.close()


# Limits for untrusted documents, on by default for every runtime load. Returns a material to draw.
func limits() -> Material:
	var over := doubling(30)
	var started := Time.get_ticks_msec()
	check(sw.compile_string(over) == null and sw.get_last_error() == "the material needs more than 4096 nodes (max_nodes)",
		"compile_string: a 2^30-node graph stops at max_nodes: " + sw.get_last_error())
	check(Time.get_ticks_msec() - started < 10000, "max_nodes stopped it in %d ms" % (Time.get_ticks_msec() - started))
	write("user://limits/over_limit.mtlx", over)
	check(sw.load_material("user://limits/over_limit.mtlx") == null and "(max_nodes)" in sw.get_last_error(),
		"load_material: max_nodes: " + sw.get_last_error())
	check(load("user://limits/over_limit.mtlx") == null, "the resource loader applies max_nodes")
	check(sw.compile_string(doubling(3), "", {"max_nodes": 0}) is ShaderMaterial, "max_nodes 0 is no limit")
	check(sw.compile_string(doubling(3), "", {"max_nodes": -1}) == null
		and sw.get_last_error().begins_with("option max_nodes: expected a count, 0 for none"),
		"a bad limit fails: " + sw.get_last_error())

	var fractal = sw.compile_string(FRACTAL)
	check(fractal is ShaderMaterial and "i < min(octaves, 16);" in fractal.shader.code,
		"fractal octaves clamped to 16 by default: " + sw.get_last_error())
	var four = sw.compile_string(FRACTAL, "", {"max_loop_iterations": 4})
	check(four is ShaderMaterial and "i < min(octaves, 4);" in four.shader.code, "max_loop_iterations 4")
	var free = sw.compile_string(FRACTAL, "", {"max_loop_iterations": 0})
	check(free is ShaderMaterial and not "min(octaves" in free.shader.code, "max_loop_iterations 0 is no clamp")

	# An include that names a non-MaterialX file fails without quoting it.
	write("user://limits/secret.txt", "<keys><key value=\"s3cr3t\"/></keys>")
	write("user://limits/mod.mtlx", """<materialx version="1.39"><xi:include href="secret.txt"/></materialx>""")
	var error := "" if sw.load_material("user://limits/mod.mtlx") else sw.get_last_error()
	check("not a MaterialX document" in error and not "s3cr3t" in error and not "key" in error,
		"an included file is never quoted: " + error)
	return fractal


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var vendored: String = args[0]
	screenshot = args[1]
	sw = Shadewire.new()

	# The editor importer (the project was imported before this run).
	var imported = load("res://smoke/textured.mtlx")
	check(textured(imported), "load(res://) gives the imported material, texture bound")

	# The runtime API from an OS path: the include and the texture resolve against the document's location.
	var os_path := ProjectSettings.globalize_path("res://smoke/textured.mtlx")
	var runtime = sw.load_material(os_path)
	check(textured(runtime), "load_material(%s) %s" % [os_path, sw.get_last_error()])

	# A vendored MaterialX example, through the runtime API and through ResourceLoader.
	var example = sw.load_material(vendored)
	check(example is ShaderMaterial, "load_material(vendored example) %s" % sw.get_last_error())
	var loaded = load(vendored)
	check(loaded is ShaderMaterial, "load(OS path) through the resource loader")

	# A document in a string, lit by MaterialX's lobes, with its base directory for includes and textures.
	var text := FileAccess.get_file_as_string("res://smoke/textured.mtlx")
	var lobes = sw.compile_string(text, os_path.get_base_dir(), {"models": "lobes", "material": "M_grid"})
	check(textured(lobes), "compile_string(text, dir, lobes) %s" % sw.get_last_error())

	check(sw.material_names(os_path) == PackedStringArray(["M_grid"]), "material_names")
	check(sw.load_material("res://missing.mtlx") == null and sw.get_last_error() != "", "a missing file fails")
	check(sw.load_material(os_path, {"transparency": "glass"}) == null, "a bad option fails: " + sw.get_last_error())
	check(sw.load_material(args[2] + "/m/remote.mtlx") == null and "load_material_async" in sw.get_last_error(),
		"load_material refuses URLs: " + sw.get_last_error())

	var materials: Array = [imported, runtime, example, lobes, limits()]
	for i in materials.size():
		sphere((i - 3) * 1.2, materials[i])
	var camera := Camera3D.new()
	camera.fov = 30
	camera.position = Vector3(0, 0, 6)
	root.add_child(camera)
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-35, -40, 0)
	root.add_child(light)
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.2, 0.2, 0.22)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color(0.35, 0.35, 0.35)
	var world := WorldEnvironment.new()
	world.environment = environment
	root.add_child(world)

	sw.material_ready.connect(func(id, material): results[id] = material)
	sw.material_failed.connect(func(id, message): results[id] = message)
	remote(args[2], args[3], args[4], args[5])


func wait(id: int) -> Variant:
	while not results.has(id):
		await process_frame
	return results[id]


func remote_load(url: String, phase: int, options := {}) -> Variant:
	options.merge({"allow_http": true, "headers": PackedStringArray(["X-Smoke-Phase: %d" % phase])})
	return await wait(sw.load_material_async(url, options))


# load_material_async from the servers: A (127.0.0.1) holds the document, B (localhost) a texture it names.
func remote(a: String, b: String, cache_dir: String, token: String) -> void:
	var url := a + "/m/remote.mtlx"
	sw.cache_dir = cache_dir
	sw.cache_max_bytes = 256 * 1024 * 1024
	check(sw.get_cache_max_bytes() == 256 * 1024 * 1024 and sw.get_cache_size() == 0, "an empty cache")
	check(sw.set_http_headers("127.0.0.1", PackedStringArray(["Authorization: " + token])), "set_http_headers")
	check(not sw.set_http_headers("127.0.0.1", PackedStringArray(["Bad Name: s3cret"]))
		and not "s3cret" in sw.get_last_error(), "a bad header is refused without echoing it: " + sw.get_last_error())
	sw.http_headers_callback = func(request_url: String) -> PackedStringArray:
		return PackedStringArray(["X-Signed: " + request_url.get_file()])
	var progress := [0, 0]
	sw.material_progress.connect(func(_id, done, total): progress[0] = done; progress[1] = total)

	var first = await remote_load(url, 1)
	check(textured(first, 3), "load_material_async(%s): include and three textures: %s" % [url, first])
	check(progress[0] > 0 and progress[0] == progress[1], "material_progress reached %d/%d" % progress)
	var size := sw.get_cache_size()
	check(size > 0, "the first load filled the cache (%d bytes)" % size)
	sw.http_headers_callback = Callable()

	var second = await remote_load(url, 2)
	check(textured(second, 3), "a second load, revalidated against the cache: %s" % second)
	var third = await remote_load(url, 3, {"cache_policy": "use"})
	check(textured(third, 3), "a cache_policy \"use\" load: %s" % third)
	var capped = await remote_load(url, 4, {"cache_policy": "use", "max_textures": 2})
	check(capped is String and capped == "the material samples 3 textures, more than 2 (max_textures)",
		"load_material_async applies max_textures: %s" % capped)

	var refused = await wait(sw.load_material_async(url))
	check(refused is String and "plain http is refused" in refused, "https only by default: %s" % refused)
	var id := sw.load_material_async(url, {"allow_http": true})
	check(sw.cancel(id) and await wait(id) == "cancelled" and not sw.cancel(id), "cancel")
	var bad = await wait(sw.load_material_async(url, {"allow_http": true, "timeout": -1}))
	check(bad is String and "option timeout" in bad, "a bad network option fails: %s" % bad)

	check(sw.evict_cache(b + "/t/white.png") and not sw.evict_cache(b + "/t/white.png"), "evict_cache")

	# Lowering the cap evicts least recently used entries down to it; 0 empties the cache.
	var files := func() -> int:
		return Array(DirAccess.get_files_at(cache_dir)).filter(func(f): return f.ends_with(".bin")).size()
	var entries: int = files.call()
	size = sw.get_cache_size()
	var cap := size - 1
	sw.cache_max_bytes = cap
	var after := sw.get_cache_size()
	check(after <= cap and after > 0 and files.call() < entries,
		"cache_max_bytes = %d evicted %d -> %d bytes, %d -> %d files" % [cap, size, after, entries, files.call()])
	sw.cache_max_bytes = 0
	check(sw.get_cache_size() == 0 and files.call() == 0, "cache_max_bytes = 0 empties the cache")
	sw.cache_max_bytes = 256 * 1024 * 1024

	sphere(3 * 1.2, first)
	remote_done = true


func _process(_delta: float) -> bool:
	frame += 1
	if not remote_done:
		if frame > 3000:
			check(false, "the remote loads finished")
		else:
			return false
	if frame < FRAMES:
		return false
	var image := root.get_viewport().get_texture().get_image()
	check(image.save_png(screenshot) == OK, "screenshot " + screenshot)
	print("FAILURES ", failures.size())
	quit(1 if failures else 0)
	return true
