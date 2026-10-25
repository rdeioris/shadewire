// The Shadewire C library (`shadewire_capi`), staged here by unreal/stage_library.py: the header under include/, the
// shared library (and on Windows its import library) under the platform's folder. The build copies the shared
// library into the plugin's own Binaries/<platform> folder, which a packaged plugin carries and the editor module
// loads it from.

using System.IO;
using UnrealBuildTool;

public class ShadewireLibrary : ModuleRules
{
	public ShadewireLibrary(ReadOnlyTargetRules Target) : base(Target)
	{
		Type = ModuleType.External;
		PublicSystemIncludePaths.Add(Path.Combine(ModuleDirectory, "include"));

		if (Target.Platform == UnrealTargetPlatform.Win64)
		{
			string Dir = Path.Combine(ModuleDirectory, "Win64");
			PublicAdditionalLibraries.Add(Path.Combine(Dir, "shadewire_capi.dll.lib"));
			PublicDelayLoadDLLs.Add("shadewire_capi.dll");
			RuntimeDependencies.Add("$(PluginDir)/Binaries/Win64/shadewire_capi.dll", Path.Combine(Dir, "shadewire_capi.dll"));
		}
		else if (Target.Platform == UnrealTargetPlatform.Mac)
		{
			string Library = Path.Combine(ModuleDirectory, "Mac", "libshadewire_capi.dylib");
			PublicDelayLoadDLLs.Add(Library);
			RuntimeDependencies.Add("$(PluginDir)/Binaries/Mac/libshadewire_capi.dylib", Library);
		}
		else if (Target.Platform == UnrealTargetPlatform.Linux)
		{
			string Library = Path.Combine(ModuleDirectory, "Linux", "libshadewire_capi.so");
			PublicAdditionalLibraries.Add(Library);
			RuntimeDependencies.Add("$(PluginDir)/Binaries/Linux/libshadewire_capi.so", Library);
		}
	}
}
