// Batch export from the command line:
//   UnrealEditor-Cmd <project> -run=ShadewireExport (-materials=/Game/A.A,/Game/B.B | -folder=/Game/Dir)
//       -out=<dir> [-format=json|mtlx|mdl] [-lobes] [-notextures]
// Writes one file per material, named after it, with its textures under <dir>/textures. `-lobes` writes MaterialX
// as a closure tree instead of `open_pbr_surface`; MDL is always one.

#pragma once

#include "Commandlets/Commandlet.h"

#include "ShadewireExportCommandlet.generated.h"

UCLASS()
class UShadewireExportCommandlet : public UCommandlet
{
	GENERATED_BODY()

public:
	UShadewireExportCommandlet();

	virtual int32 Main(const FString& Params) override;
};
