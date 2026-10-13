extends SceneTree

# Compiles and draws every res://shaders/*.gdshader with Godot's real renderer: one frame each on a quad in front of
# the camera, under a directional light (so light() runs too). Godot logs "SHADER ERROR" for a shader that fails;
# check_gdshader.py reads the log. The headless (dummy) renderer is not enough: it accepts code the renderers refuse.

var files = []
var index = 0
var mesh

func _init():
	for f in DirAccess.open("res://shaders").get_files():
		if f.ends_with(".gdshader"):
			files.append(f)
	var light = DirectionalLight3D.new()
	root.add_child(light)
	mesh = MeshInstance3D.new()
	mesh.mesh = QuadMesh.new()
	root.add_child(mesh)
	var cam = Camera3D.new()
	cam.position = Vector3(0, 0, 2)
	root.add_child(cam)
	cam.make_current()

func _process(_delta):
	if index >= files.size():
		print("CHECKED ", files.size())
		quit()
		return false
	var f = files[index]
	print("CHECK ", f)
	var shader = Shader.new()
	shader.code = FileAccess.get_file_as_string("res://shaders/" + f)
	var material = ShaderMaterial.new()
	material.shader = shader
	mesh.material_override = material
	index += 1
	return false
