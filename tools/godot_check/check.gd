extends SceneTree

# Compiles every res://shaders/*.gdshader with Godot's shader compiler. Under --headless the dummy
# renderer still runs the full compiler and logs "SHADER ERROR" on failure; check_gdshader.py
# reads the log.

func _init():
	var count = 0
	for f in DirAccess.open("res://shaders").get_files():
		if not f.ends_with(".gdshader"):
			continue
		print("CHECK ", f)
		var shader = Shader.new()
		shader.code = FileAccess.get_file_as_string("res://shaders/" + f)
		count += 1
	print("CHECKED ", count)
	quit()
