# Loads materials every way the extension offers, draws them on spheres with the real renderer, saves a
# screenshot and quits; exits 1 when a check fails. Arguments after "--": <vendored .mtlx> <screenshot .png>.
extends SceneTree

const FRAMES := 30

var failures: Array[String] = []
var frame := 0
var screenshot := ""


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


func textured(material: Material) -> bool:
	return material is ShaderMaterial and material.get_shader_parameter("shadewire_t0") is Texture2D


func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var vendored: String = args[0]
	screenshot = args[1]
	var sw := Shadewire.new()

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

	var materials: Array = [imported, runtime, example, lobes]
	for i in materials.size():
		sphere((i - 1.5) * 1.2, materials[i])
	var camera := Camera3D.new()
	camera.position = Vector3(0, 0, 2.2)
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


func _process(_delta: float) -> bool:
	frame += 1
	if frame < FRAMES:
		return false
	var image := root.get_viewport().get_texture().get_image()
	check(image.save_png(screenshot) == OK, "screenshot " + screenshot)
	print("FAILURES ", failures.size())
	quit(1 if failures else 0)
	return true
