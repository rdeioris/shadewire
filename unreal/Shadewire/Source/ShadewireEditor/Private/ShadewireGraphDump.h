// The `shadewire-unreal-graph` document: Unreal materials as the editor holds them, nothing interpreted.

#pragma once

#include "CoreMinimal.h"

class UMaterialInterface;

struct FShadewireDumpOptions
{
	/** The folder the document will be written to; texture paths are relative to it. */
	FString DocumentDir;
	/** Where texture files are written, under DocumentDir; empty writes none and leaves texture `file`s empty. */
	FString TexturesDir;
};

struct FShadewireDump
{
	FString Text;
	TArray<FString> Warnings;
};

/** One document holding `Materials`, the parents of the instances among them, every material function and texture
 *  they reach; textures are written as files when the options say where. */
FShadewireDump ShadewireDumpGraph(TConstArrayView<UMaterialInterface*> Materials, const FShadewireDumpOptions& Options);
