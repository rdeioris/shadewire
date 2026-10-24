// The Shadewire editor module: loads the library and adds Export MaterialX, Export MDL and Save Graph to the
// context menus of materials and material instances in the Content Browser.

#include "ShadewireExport.h"

#include "ContentBrowserMenuContexts.h"
#include "DesktopPlatformModule.h"
#include "Framework/Application/SlateApplication.h"
#include "Framework/Notifications/NotificationManager.h"
#include "IDesktopPlatform.h"
#include "Logging/MessageLog.h"
#include "Materials/Material.h"
#include "Materials/MaterialInstanceConstant.h"
#include "MessageLogModule.h"
#include "Misc/Paths.h"
#include "Modules/ModuleManager.h"
#include "ToolMenus.h"
#include "Widgets/Notifications/SNotificationList.h"

#define LOCTEXT_NAMESPACE "Shadewire"

namespace
{
	const FName LogName(TEXT("Shadewire"));

	/** The folder the last export went to, offered again by the next one. */
	FString LastFolder;

	void Notify(const FText& Text, bool bSuccess)
	{
		FNotificationInfo Info(Text);
		Info.ExpireDuration = 5.0f;
		TSharedPtr<SNotificationItem> Item = FSlateNotificationManager::Get().AddNotification(Info);
		if (Item)
		{
			Item->SetCompletionState(bSuccess ? SNotificationItem::CS_Success : SNotificationItem::CS_Fail);
		}
	}

	bool PickFolder(FString& OutFolder)
	{
		IDesktopPlatform* Desktop = FDesktopPlatformModule::Get();
		if (!Desktop)
		{
			return false;
		}
		const void* Parent = FSlateApplication::Get().FindBestParentWindowHandleForDialogs(nullptr);
		const FString Default = LastFolder.IsEmpty() ? FPaths::ProjectSavedDir() : LastFolder;
		if (!Desktop->OpenDirectoryDialog(Parent, TEXT("Export to folder"), Default, OutFolder))
		{
			return false;
		}
		LastFolder = OutFolder;
		return true;
	}

	/** Exports the context's materials in `Format` to a folder the user picks, reporting to the Message Log. */
	void ExportSelected(const FToolMenuContext& MenuContext, EShadewireFormat Format)
	{
		const UContentBrowserAssetContextMenuContext* Context = MenuContext.FindContext<UContentBrowserAssetContextMenuContext>();
		if (!Context)
		{
			return;
		}
		const TArray<UMaterialInterface*> Materials = Context->LoadSelectedObjects<UMaterialInterface>();
		FShadewireExportOptions Options;
		Options.Format = Format;
		if (Materials.Num() == 0 || !PickFolder(Options.OutDir))
		{
			return;
		}

		FMessageLog Log(LogName);
		Log.NewPage(FText::Format(LOCTEXT("Page", "Export {0}"), FText::FromString(ShadewireExtension(Format))));
		int32 Written = 0;
		for (UMaterialInterface* Material : Materials)
		{
			const FShadewireExportResult Result = ShadewireExport(Material, Options);
			for (const FString& Warning : Result.Warnings)
			{
				Log.Warning(FText::FromString(FString::Printf(TEXT("%s: %s"), *Material->GetName(), *Warning)));
			}
			if (Result.bOk)
			{
				Log.Info(FText::FromString(FString::Printf(TEXT("%s: wrote %s"), *Material->GetName(), *Result.Path)));
				++Written;
			}
			else
			{
				Log.Error(FText::FromString(FString::Printf(TEXT("%s: %s"), *Material->GetName(), *Result.Error)));
			}
		}
		const bool bAll = Written == Materials.Num();
		Notify(FText::Format(LOCTEXT("Done", "Shadewire: {0} of {1} exported"), Written, Materials.Num()), bAll);
		if (!bAll || Log.NumMessages(EMessageSeverity::Warning) > 0)
		{
			Log.Open(EMessageSeverity::Warning);
		}
	}

	void AddEntry(FToolMenuSection& Section, FName Name, const FText& Label, const FText& Tip, EShadewireFormat Format)
	{
		Section.AddMenuEntry(Name, Label, Tip, FSlateIcon(),
			FToolMenuExecuteAction::CreateLambda([Format](const FToolMenuContext& Context) { ExportSelected(Context, Format); }));
	}

	void RegisterMenus()
	{
		const UClass* Classes[] = {UMaterial::StaticClass(), UMaterialInstanceConstant::StaticClass()};
		for (UToolMenu* Menu : UE::ContentBrowser::ExtendToolMenu_AssetContextMenus(TConstArrayView<UClass*>((UClass**)Classes, 2)))
		{
			FToolMenuSection& Section = Menu->FindOrAddSection(TEXT("Shadewire"), LOCTEXT("Section", "Shadewire"));
			AddEntry(Section, TEXT("ShadewireMaterialX"), LOCTEXT("MaterialX", "Export MaterialX..."),
				LOCTEXT("MaterialXTip", "Writes the material as a MaterialX document, with its textures"), EShadewireFormat::MaterialX);
			AddEntry(Section, TEXT("ShadewireMdl"), LOCTEXT("Mdl", "Export MDL..."),
				LOCTEXT("MdlTip", "Writes the material as an MDL 1.7 module, with its textures"), EShadewireFormat::Mdl);
			AddEntry(Section, TEXT("ShadewireGraph"), LOCTEXT("Graph", "Save Shadewire Graph..."),
				LOCTEXT("GraphTip", "Writes the material's graph as the JSON document Shadewire reads"), EShadewireFormat::Graph);
		}
	}
}

class FShadewireEditorModule : public IModuleInterface
{
public:
	virtual void StartupModule() override
	{
		ShadewireLoadLibrary();
		FMessageLogModule& Logs = FModuleManager::LoadModuleChecked<FMessageLogModule>("MessageLog");
		FMessageLogInitializationOptions LogOptions;
		LogOptions.bShowPages = true;
		Logs.RegisterLogListing(LogName, LOCTEXT("LogLabel", "Shadewire"), LogOptions);
		if (!IsRunningCommandlet())
		{
			UToolMenus::RegisterStartupCallback(FSimpleMulticastDelegate::FDelegate::CreateStatic(&RegisterMenus));
		}
	}

	virtual void ShutdownModule() override
	{
		UToolMenus::UnRegisterStartupCallback(this);
		UToolMenus::UnregisterOwner(this);
		if (FModuleManager::Get().IsModuleLoaded("MessageLog"))
		{
			FModuleManager::GetModuleChecked<FMessageLogModule>("MessageLog").UnregisterLogListing(LogName);
		}
		ShadewireUnloadLibrary();
	}
};

IMPLEMENT_MODULE(FShadewireEditorModule, ShadewireEditor)

#undef LOCTEXT_NAMESPACE
