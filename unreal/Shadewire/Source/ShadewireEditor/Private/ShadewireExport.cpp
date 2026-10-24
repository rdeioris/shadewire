#include "ShadewireExport.h"

#include "ShadewireGraphDump.h"

#include "HAL/PlatformProcess.h"
#include "Interfaces/IPluginManager.h"
#include "Materials/MaterialInterface.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Modules/ModuleManager.h"

#include "shadewire.h"

namespace
{
	void* LibraryHandle = nullptr;

	const TCHAR* LibraryFile()
	{
#if PLATFORM_WINDOWS
		return TEXT("shadewire_capi.dll");
#elif PLATFORM_MAC
		return TEXT("libshadewire_capi.dylib");
#else
		return TEXT("libshadewire_capi.so");
#endif
	}

	/** The error's message, freeing it. */
	FString TakeError(shadewire_error* Error)
	{
		if (!Error)
		{
			return TEXT("unknown error");
		}
		size_t Length = 0;
		const char* Message = shadewire_error_message(Error, &Length);
		FString Text = Message ? FString(FUTF8ToTCHAR(Message, (int32)Length)) : FString(TEXT("unknown error"));
		shadewire_error_free(Error);
		return Text;
	}

	/** Every string of `Strings`, freeing it. */
	void TakeStrings(shadewire_strings* Strings, TArray<FString>& Out)
	{
		for (size_t i = 0, n = shadewire_strings_count(Strings); i < n; ++i)
		{
			Out.Add(FString(UTF8_TO_TCHAR(shadewire_strings_get(Strings, i))));
		}
		shadewire_strings_free(Strings);
	}

	bool Save(const FString& Text, const FString& Path, FShadewireExportResult& Result)
	{
		Result.Path = Path;
		if (!FFileHelper::SaveStringToFile(Text, *Path, FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM))
		{
			Result.Error = FString::Printf(TEXT("could not write %s"), *Path);
			return false;
		}
		return true;
	}
}

void ShadewireLoadLibrary()
{
	if (LibraryHandle)
	{
		return;
	}
	// The build copies the library next to this module, which is the plugin's binaries or, for a plugin a project
	// lists in AdditionalPluginDirectories, the project's.
	const FString Module = FModuleManager::Get().GetModuleFilename(TEXT("ShadewireEditor"));
	if (!Module.IsEmpty())
	{
		LibraryHandle = FPlatformProcess::GetDllHandle(*FPaths::Combine(FPaths::GetPath(Module), LibraryFile()));
	}
	if (TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("Shadewire")); !LibraryHandle && Plugin)
	{
		const FString Path = FPaths::Combine(Plugin->GetBaseDir(), TEXT("Binaries"), FPlatformProcess::GetBinariesSubdirectory(), LibraryFile());
		LibraryHandle = FPlatformProcess::GetDllHandle(*Path);
	}
}

void ShadewireUnloadLibrary()
{
	if (LibraryHandle)
	{
		FPlatformProcess::FreeDllHandle(LibraryHandle);
		LibraryHandle = nullptr;
	}
}

bool ShadewireLibraryReady(FString& OutWhy)
{
	if (!LibraryHandle)
	{
		OutWhy = FString::Printf(TEXT("%s was not found in the plugin's binaries (run unreal/stage_library.py and rebuild)"), LibraryFile());
		return false;
	}
	const uint32 Version = shadewire_version();
	const uint32 Major = Version >> 16;
	const uint32 Minor = (Version >> 8) & 0xff;
	if (Major != SHADEWIRE_VERSION_MAJOR || Minor != SHADEWIRE_VERSION_MINOR)
	{
		OutWhy = FString::Printf(TEXT("the library is Shadewire %s, the plugin needs %d.%d"),
			UTF8_TO_TCHAR(shadewire_version_string()), SHADEWIRE_VERSION_MAJOR, SHADEWIRE_VERSION_MINOR);
		return false;
	}
	return true;
}

const TCHAR* ShadewireExtension(EShadewireFormat Format)
{
	switch (Format)
	{
	case EShadewireFormat::Graph: return TEXT("json");
	case EShadewireFormat::Mdl: return TEXT("mdl");
	default: return TEXT("mtlx");
	}
}

FShadewireExportResult ShadewireExport(UMaterialInterface* Material, const FShadewireExportOptions& Options)
{
	FShadewireExportResult Result;
	FShadewireDumpOptions DumpOptions;
	DumpOptions.DocumentDir = Options.OutDir;
	if (Options.bTextures)
	{
		DumpOptions.TexturesDir = Options.OutDir / TEXT("textures");
	}
	const FShadewireDump Dump = ShadewireDumpGraph({Material}, DumpOptions);
	Result.Warnings = Dump.Warnings;
	const FString Path = Options.OutDir / FString::Printf(TEXT("%s.%s"), *Material->GetName(), ShadewireExtension(Options.Format));

	if (Options.Format == EShadewireFormat::Graph)
	{
		Result.bOk = Save(Dump.Text, Path, Result);
		return Result;
	}
	FString Why;
	if (!ShadewireLibraryReady(Why))
	{
		Result.Error = Why;
		return Result;
	}

	const FTCHARToUTF8 Text(*Dump.Text);
	const FTCHARToUTF8 Name(*Material->GetName());
	shadewire_ir_options IrOptions;
	shadewire_ir_options_default(&IrOptions);
	const char* Lobes[] = {"surface", "surface_unlit"};
	if (Options.Format == EShadewireFormat::Mdl || Options.bLobes)
	{
		IrOptions.models = Lobes;
		IrOptions.model_count = UE_ARRAY_COUNT(Lobes);
	}

	shadewire_ir* Ir = nullptr;
	shadewire_error* Error = nullptr;
	if (shadewire_unreal_to_ir(Text.Get(), Text.Length(), Name.Get(), &IrOptions, &Ir, &Error) != SHADEWIRE_OK)
	{
		Result.Error = TakeError(Error);
		return Result;
	}
	shadewire_strings* Warnings = nullptr;
	if (shadewire_ir_warnings(Ir, &Warnings, nullptr) == SHADEWIRE_OK)
	{
		TakeStrings(Warnings, Result.Warnings);
	}

	if (Options.Format == EShadewireFormat::MaterialX)
	{
		shadewire_document* Document = nullptr;
		if (shadewire_ir_to_mtlx(Ir, false, Name.Get(), &Document, &Error) != SHADEWIRE_OK)
		{
			Result.Error = TakeError(Error);
		}
		else
		{
			for (size_t i = 0, n = shadewire_document_warning_count(Document); i < n; ++i)
			{
				Result.Warnings.Add(FString(UTF8_TO_TCHAR(shadewire_document_warning(Document, i))));
			}
			size_t Length = 0;
			const char* Mtlx = shadewire_document_text(Document, &Length);
			Result.bOk = Save(FString(FUTF8ToTCHAR(Mtlx, (int32)Length)), Path, Result);
			shadewire_document_free(Document);
		}
	}
	else
	{
		shadewire_compile_options CompileOptions;
		shadewire_compile_options_default(&CompileOptions);
		shadewire_target Target{SHADEWIRE_TARGET_MDL, 0};
		shadewire_compiled* Compiled = nullptr;
		if (shadewire_compile(Ir, Target, &CompileOptions, &Compiled, &Error) != SHADEWIRE_OK)
		{
			Result.Error = TakeError(Error);
		}
		else
		{
			size_t Length = 0;
			const char* Code = shadewire_compiled_code(Compiled, &Length);
			Result.bOk = Save(FString(FUTF8ToTCHAR(Code, (int32)Length)), Path, Result);
			shadewire_compiled_free(Compiled);
		}
	}
	shadewire_ir_free(Ir);
	return Result;
}
