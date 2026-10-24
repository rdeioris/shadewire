#include "ShadewireExportCommandlet.h"

#include "ShadewireExport.h"

#include "AssetRegistry/AssetRegistryModule.h"
#include "AssetRegistry/IAssetRegistry.h"
#include "Materials/MaterialInterface.h"
#include "Misc/Paths.h"

#include UE_INLINE_GENERATED_CPP_BY_NAME(ShadewireExportCommandlet)

DEFINE_LOG_CATEGORY_STATIC(LogShadewire, Log, All);

UShadewireExportCommandlet::UShadewireExportCommandlet()
{
	IsClient = false;
	IsEditor = true;
	IsServer = false;
	LogToConsole = true;
}

namespace
{
	/** The materials named by `-materials=` or found under `-folder=`, sorted by path. */
	TArray<UMaterialInterface*> FindMaterials(const TMap<FString, FString>& Params)
	{
		TArray<UMaterialInterface*> Materials;
		if (const FString* List = Params.Find(TEXT("materials")))
		{
			TArray<FString> Paths;
			List->ParseIntoArray(Paths, TEXT(","));
			for (const FString& Path : Paths)
			{
				if (UMaterialInterface* Material = LoadObject<UMaterialInterface>(nullptr, *Path))
				{
					Materials.Add(Material);
				}
				else
				{
					UE_LOG(LogShadewire, Error, TEXT("No material at %s"), *Path);
				}
			}
		}
		if (const FString* Folder = Params.Find(TEXT("folder")))
		{
			IAssetRegistry& Registry = FAssetRegistryModule::GetRegistry();
			Registry.SearchAllAssets(true);
			TArray<FAssetData> Assets;
			Registry.GetAssetsByPath(FName(**Folder), Assets, true);
			Assets.Sort([](const FAssetData& A, const FAssetData& B) { return A.GetObjectPathString() < B.GetObjectPathString(); });
			for (const FAssetData& Asset : Assets)
			{
				if (UMaterialInterface* Material = Cast<UMaterialInterface>(Asset.GetAsset()))
				{
					Materials.Add(Material);
				}
			}
		}
		return Materials;
	}
}

int32 UShadewireExportCommandlet::Main(const FString& CommandLine)
{
	TArray<FString> Tokens;
	TArray<FString> Switches;
	TMap<FString, FString> Params;
	ParseCommandLine(*CommandLine, Tokens, Switches, Params);

	const FString* Out = Params.Find(TEXT("out"));
	if (!Out)
	{
		UE_LOG(LogShadewire, Error, TEXT("-out=<dir> is required"));
		return 1;
	}
	FShadewireExportOptions Options;
	const FString Format = Params.FindRef(TEXT("format"), TEXT("json"));
	if (Format == TEXT("json"))
	{
		Options.Format = EShadewireFormat::Graph;
	}
	else if (Format == TEXT("mtlx"))
	{
		Options.Format = EShadewireFormat::MaterialX;
	}
	else if (Format == TEXT("mdl"))
	{
		Options.Format = EShadewireFormat::Mdl;
	}
	else
	{
		UE_LOG(LogShadewire, Error, TEXT("-format=%s: json, mtlx or mdl"), *Format);
		return 1;
	}
	Options.OutDir = FPaths::ConvertRelativePathToFull(*Out);
	Options.bTextures = !Switches.Contains(TEXT("notextures"));
	Options.bLobes = Switches.Contains(TEXT("lobes"));

	const TArray<UMaterialInterface*> Materials = FindMaterials(Params);
	if (Materials.Num() == 0)
	{
		UE_LOG(LogShadewire, Error, TEXT("No materials: pass -materials=<paths> or -folder=<content folder>"));
		return 1;
	}

	int32 Failures = 0;
	for (UMaterialInterface* Material : Materials)
	{
		const FShadewireExportResult Result = ShadewireExport(Material, Options);
		for (const FString& Warning : Result.Warnings)
		{
			UE_LOG(LogShadewire, Warning, TEXT("%s: %s"), *Material->GetName(), *Warning);
		}
		if (Result.bOk)
		{
			UE_LOG(LogShadewire, Display, TEXT("Wrote %s"), *Result.Path);
		}
		else
		{
			UE_LOG(LogShadewire, Error, TEXT("%s: %s"), *Material->GetName(), *Result.Error);
			++Failures;
		}
	}
	return Failures ? 1 : 0;
}
