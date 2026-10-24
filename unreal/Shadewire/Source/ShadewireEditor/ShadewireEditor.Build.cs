// The Shadewire editor module: dumps material graphs and exports them through the Shadewire library.

using UnrealBuildTool;

public class ShadewireEditor : ModuleRules
{
	public ShadewireEditor(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		PrivateDependencyModuleNames.AddRange(new string[] {
			"Core",
			"CoreUObject",
			"Engine",
			"RenderCore",
			"UnrealEd",
			"AssetRegistry",
			"ImageCore",
			"Projects",
			"Slate",
			"SlateCore",
			"ToolMenus",
			"ContentBrowser",
			"DesktopPlatform",
			"MessageLog",
			"ShadewireLibrary",
		});
	}
}
