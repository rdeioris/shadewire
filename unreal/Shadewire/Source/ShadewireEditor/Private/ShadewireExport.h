// Exporting a material: its graph dumped, then (for MaterialX and MDL) lowered and written by the Shadewire library.

#pragma once

#include "CoreMinimal.h"

class UMaterialInterface;

enum class EShadewireFormat : uint8
{
	/** The `shadewire-unreal-graph` document itself. */
	Graph,
	MaterialX,
	/** An MDL 1.7 module; MDL takes closure trees, so the material always opens into its lobes. */
	Mdl,
};

struct FShadewireExportOptions
{
	EShadewireFormat Format = EShadewireFormat::MaterialX;
	/** The folder the file is written to; textures go to its `textures` folder unless TexturesDir says otherwise. */
	FString OutDir;
	/** The file's name without its extension; empty for the material's name. */
	FString BaseName;
	/** Where textures are written; empty for OutDir's `textures` folder. */
	FString TexturesDir;
	/** Write textures (the document then names them relative to OutDir). */
	bool bTextures = true;
	/** MaterialX: open the material into its closure tree instead of `open_pbr_surface`. */
	bool bLobes = false;
};

struct FShadewireExportResult
{
	bool bOk = false;
	/** The file written. */
	FString Path;
	/** MaterialX: the document's `surfacematerial` name (the material's name, made a valid MaterialX name). */
	FString DocumentName;
	TArray<FString> Warnings;
	FString Error;
};

/** Whether the Shadewire library is loaded and of this plugin's version; `OutWhy` says why not. */
bool ShadewireLibraryReady(FString& OutWhy);

/** Loads the library from the plugin's binaries; call once at startup. */
void ShadewireLoadLibrary();
void ShadewireUnloadLibrary();

FShadewireExportResult ShadewireExport(UMaterialInterface* Material, const FShadewireExportOptions& Options);

/** The extension of a format's files, without the dot. */
const TCHAR* ShadewireExtension(EShadewireFormat Format);
