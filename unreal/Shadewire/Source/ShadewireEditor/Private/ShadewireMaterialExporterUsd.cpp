#include "ShadewireMaterialExporterUsd.h"

#include "ShadewireExport.h"
#include "ShadewireSettings.h"

#include "AssetExportTask.h"
#include "MaterialExporterUSD.h"
#include "MaterialExporterUSDOptions.h"
#include "Materials/Material.h"
#include "Materials/MaterialInstance.h"
#include "MaterialShared.h"
#include "Misc/EngineVersion.h"
#include "Misc/Paths.h"
#include "UnrealUSDWrapper.h"
#include "USDConversionUtils.h"
#include "USDProjectSettings.h"
#include "USDShadeConversion.h"
#include "USDUnrealAssetInfo.h"
#include "UsdWrappers/SdfLayer.h"
#include "UsdWrappers/SdfPath.h"
#include "UsdWrappers/UsdPrim.h"
#include "UsdWrappers/UsdStage.h"

#include UE_INLINE_GENERATED_CPP_BY_NAME(ShadewireMaterialExporterUsd)

DEFINE_LOG_CATEGORY_STATIC(LogShadewireUsd, Log, All);

namespace
{
	/** `Material` as the USD layer at `FilePath`: a `.mtlx` of the same name beside it, and the layer's default prim
	 *  a Material that references the document's material and keeps Unreal's own surface output. */
	bool ExportMaterialX(const UMaterialInterface& Material, const UMaterialExporterUSDOptions& Options, const FString& FilePath, FString& OutError)
	{
#if USE_USD_SDK
		const FString Dir = FPaths::GetPath(FilePath);
		FShadewireExportOptions Export;
		Export.Format = EShadewireFormat::MaterialX;
		Export.OutDir = Dir;
		Export.BaseName = FPaths::GetBaseFilename(FilePath);
		Export.TexturesDir = Options.MaterialBakingOptions.TexturesDir.Path;
		Export.bLobes = GetDefault<UShadewireSettings>()->bUsdMaterialXLobes;
		const FShadewireExportResult Result = ShadewireExport(const_cast<UMaterialInterface*>(&Material), Export);
		for (const FString& Warning : Result.Warnings)
		{
			UE_LOG(LogShadewireUsd, Warning, TEXT("%s: %s"), *Material.GetName(), *Warning);
		}
		if (!Result.bOk || Result.DocumentName.IsEmpty())
		{
			OutError = Result.Error.IsEmpty() ? TEXT("the MaterialX document names no material") : Result.Error;
			return false;
		}

		UE::FUsdStage Stage = UnrealUSDWrapper::NewStage(*FilePath);
		if (!Stage)
		{
			OutError = FString::Printf(TEXT("could not create %s"), *FilePath);
			return false;
		}
		const FString RootPath = TEXT("/") + UsdUtils::SanitizeUsdIdentifier(*Material.GetName());
		UE::FUsdPrim Root = Stage.DefinePrim(UE::FSdfPath(*RootPath), TEXT("Material"));
		if (!Root)
		{
			OutError = FString::Printf(TEXT("could not define %s"), *RootPath);
			return false;
		}
		Stage.SetDefaultPrim(Root);
		UsdUtils::SetUnrealSurfaceOutput(Root, Material.GetPathName());
		const UE::FSdfPath Target(*(TEXT("/MaterialX/Materials/") + Result.DocumentName));
		UsdUtils::AddReference(Root, *Result.Path, EReferencerTypeHandling::Ignore, Target);

		if (Options.MetadataOptions.bExportAssetInfo)
		{
			FUsdUnrealAssetInfo Info;
			Info.Name = Material.GetName();
			Info.Identifier = FilePath;
			Info.UnrealContentPath = Material.GetPathName();
			Info.UnrealAssetType = Material.GetClass()->GetName();
			Info.UnrealExportTime = FDateTime::Now().ToString();
			Info.UnrealEngineVersion = FEngineVersion::Current().ToString();
			UsdUtils::SetPrimAssetInfo(Root, Info);
		}
		Stage.GetRootLayer().Save();
		return true;
#else
		OutError = TEXT("this editor has no USD SDK");
		return false;
#endif
	}
}

UShadewireMaterialExporterUsd::UShadewireMaterialExporterUsd()
{
#if USE_USD_SDK
	UnrealUSDWrapper::AddUsdExportFileFormatDescriptions(FormatExtension, FormatDescription);
#endif
	SupportedClass = UMaterial::StaticClass();
	bText = false;
}

UShadewireMaterialInstanceExporterUsd::UShadewireMaterialInstanceExporterUsd()
{
	SupportedClass = UMaterialInstance::StaticClass();
}

bool UShadewireMaterialExporterUsd::ExportBinary(UObject* Object, const TCHAR* Type, FArchive& Ar, FFeedbackContext* Warn, int32 FileIndex, uint32 PortFlags)
{
	UMaterialInterface* Material = Cast<UMaterialInterface>(Object);
	if (!Material)
	{
		return false;
	}
	UMaterialExporterUSDOptions* Options = ExportTask ? Cast<UMaterialExporterUSDOptions>(ExportTask->Options) : nullptr;
	if (!Options)
	{
		Options = GetMutableDefault<UMaterialExporterUSDOptions>();
	}
	if (ExportTask)
	{
		ExportTask->bPrompt = false;
	}
	const FString FilePath = UExporter::CurrentFilename;

	if (GetDefault<UShadewireSettings>()->bUsdMaterialsAsMaterialX)
	{
		FString Error;
		if (ExportMaterialX(*Material, *Options, FilePath, Error))
		{
			return true;
		}
		UE_LOG(LogShadewireUsd, Warning, TEXT("%s: not exported as MaterialX (%s), baked instead"), *Material->GetName(), *Error);
	}

	// Unreal's baking, as its own exporter does it.
	if (UMaterial* Parent = Material->GetMaterial())
	{
		if (!Parent->GetMaterialResource(GMaxRHIShaderPlatform))
		{
			Parent->ForceRecompileForRendering();
		}
	}
	return UMaterialExporterUsd::ExportMaterial(
		*Material,
		Options->MaterialBakingOptions,
		Options->MetadataOptions,
		FFilePath{FilePath},
		ExportTask ? ExportTask->bReplaceIdentical : true,
		Options->bReExportIdenticalAssets,
		ExportTask ? ExportTask->bAutomated : false
	);
}
