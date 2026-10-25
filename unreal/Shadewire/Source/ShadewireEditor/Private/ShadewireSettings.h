// The plugin's settings, under Editor Preferences > Plugins > Shadewire.

#pragma once

#include "Engine/DeveloperSettings.h"

#include "ShadewireSettings.generated.h"

UCLASS(config = EditorPerProjectUserSettings, meta = (DisplayName = "Shadewire"))
class UShadewireSettings : public UDeveloperSettings
{
	GENERATED_BODY()

public:
	/** USD export writes materials as Shadewire MaterialX (a `.mtlx` beside each material layer, which references
	 *  it) instead of baking them to textures. Applies when the export bakes materials ("Bake Materials" on). */
	UPROPERTY(config, EditAnywhere, Category = "USD Export")
	bool bUsdMaterialsAsMaterialX = true;

	/** Those MaterialX documents open every material into its closure tree instead of `open_pbr_surface`. */
	UPROPERTY(config, EditAnywhere, Category = "USD Export", meta = (EditCondition = "bUsdMaterialsAsMaterialX"))
	bool bUsdMaterialXLobes = false;

	virtual FName GetCategoryName() const override { return TEXT("Plugins"); }
};
