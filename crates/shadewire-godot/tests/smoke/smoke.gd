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

	var materials: Array = [imported, runtime, example, lobes]
	for i in materials.size():
		sphere((i - 2) * 1.2, materials[i])
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

	sphere(2 * 1.2, first)
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
