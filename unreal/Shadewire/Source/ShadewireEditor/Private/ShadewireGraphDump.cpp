#include "ShadewireGraphDump.h"

#include "ShadewireJson.h"

#include "Engine/Texture.h"
#include "Engine/Texture2D.h"
#include "HAL/FileManager.h"
#include "ImageCore.h"
#include "ImageUtils.h"
#include "Materials/Material.h"
#include "Materials/MaterialExpression.h"
#include "Materials/MaterialFunction.h"
#include "Materials/MaterialFunctionInterface.h"
#include "Materials/MaterialInstance.h"
#include "Misc/EngineVersion.h"
#include "Misc/Paths.h"
#include "RenderUtils.h"
#include "UObject/UnrealType.h"

namespace
{
	const TCHAR* Format = TEXT("shadewire-unreal-graph");
	constexpr int32 Version = 1;

	/** The material's settings that bear on shading, by `UPROPERTY` name; absent ones are left out. */
	const TCHAR* const MaterialSettings[] = {
		TEXT("MaterialDomain"),
		TEXT("BlendMode"),
		TEXT("ShadingModel"),
		TEXT("TwoSided"),
		TEXT("bTangentSpaceNormal"),
		TEXT("bUseMaterialAttributes"),
		TEXT("OpacityMaskClipValue"),
		TEXT("TranslucencyLightingMode"),
		TEXT("bIsThinSurface"),
	};

	/** Structs written as one-line arrays of their members. */
	const TCHAR* const VectorStructs[] = {
		TEXT("LinearColor"), TEXT("Color"), TEXT("Vector"), TEXT("Vector2D"), TEXT("Vector4"),
		TEXT("Vector3f"), TEXT("Vector2f"), TEXT("Vector4f"), TEXT("Vector3d"), TEXT("Vector4d"),
		TEXT("IntPoint"), TEXT("IntVector"),
	};

	bool StructIsNamed(const UScriptStruct* Struct, const TCHAR* Name)
	{
		for (const UStruct* S = Struct; S; S = S->GetSuperStruct())
		{
			if (S->GetFName() == Name)
			{
				return true;
			}
		}
		return false;
	}

	/** Properties the dump leaves out: transient and deprecated ones, GUIDs, and expression inputs (the inputs
	 *  list holds those). */
	bool Skipped(const FProperty* Property)
	{
		if (Property->HasAnyPropertyFlags(CPF_Transient | CPF_Deprecated))
		{
			return true;
		}
		const FProperty* Inner = Property;
		if (const FArrayProperty* Array = CastField<FArrayProperty>(Property))
		{
			Inner = Array->Inner;
		}
		if (const FStructProperty* Struct = CastField<FStructProperty>(Inner))
		{
			return StructIsNamed(Struct->Struct, TEXT("ExpressionInput")) || Struct->Struct->GetFName() == TEXT("Guid");
		}
		return false;
	}

	/** A name as text, the null name (`None`) as an empty string. */
	FString NameText(FName Name)
	{
		return Name.IsNone() ? FString() : Name.ToString();
	}

	/** `[R, G, B, A]` of a set mask, else null. */
	void Mask(int32 Mask, int32 R, int32 G, int32 B, int32 A, FShadewireJson& J)
	{
		if (!Mask)
		{
			J.Null();
			return;
		}
		J.BeginArray(true);
		J.Int(R);
		J.Int(G);
		J.Int(B);
		J.Int(A);
		J.EndArray();
	}

	class FDumper
	{
	public:
		explicit FDumper(const FShadewireDumpOptions& InOptions) : Options(InOptions) {}

		FShadewireDump Run(TConstArrayView<UMaterialInterface*> Materials)
		{
			TArray<UMaterialInterface*> Parents;
			for (UMaterialInterface* Material : Materials)
			{
				for (UMaterialInstance* Instance = Cast<UMaterialInstance>(Material); Instance;
					 Instance = Cast<UMaterialInstance>(Instance->Parent))
				{
					UMaterialInterface* Parent = Instance->Parent;
					if (!Parent)
					{
						Warn(FString::Printf(TEXT("%s: material instance without a parent"), *Instance->GetPathName()));
					}
					else if (!Materials.Contains(Parent))
					{
						Parents.AddUnique(Parent);
					}
				}
			}

			J.BeginObject();
			J.Key(TEXT("format"));
			J.String(Format);
			J.Key(TEXT("version"));
			J.Int(Version);
			J.Key(TEXT("producer"));
			J.String(FString::Printf(TEXT("Unreal Engine %s"), *FEngineVersion::Current().ToString(EVersionComponent::Patch)));
			J.Key(TEXT("substrate"));
			J.Bool(Substrate::IsSubstrateEnabled());
			J.Key(TEXT("materials"));
			WriteMaterials(Materials);
			J.Key(TEXT("parents"));
			WriteMaterials(Parents);
			J.Key(TEXT("functions"));
			J.BeginArray();
			for (int32 i = 0; i < Functions.Num(); ++i)
			{
				WriteFunction(Functions[i]);
			}
			J.EndArray();
			J.Key(TEXT("textures"));
			J.BeginArray();
			for (UTexture* Texture : Textures)
			{
				WriteTexture(Texture);
			}
			J.EndArray();
			J.EndObject();

			FShadewireDump Dump;
			Dump.Text = J.Text() + TEXT("\n");
			Dump.Warnings = MoveTemp(Warnings);
			return Dump;
		}

	private:
		const FShadewireDumpOptions& Options;
		FShadewireJson J;
		TArray<UMaterialFunctionInterface*> Functions;
		TArray<UTexture*> Textures;
		TSet<FString> TextureFiles;
		TArray<FString> Warnings;

		void Warn(const FString& Warning)
		{
			Warnings.AddUnique(Warning);
		}

		void WriteMaterials(TConstArrayView<UMaterialInterface*> Materials)
		{
			J.BeginArray();
			for (UMaterialInterface* Material : Materials)
			{
				if (UMaterial* Base = Cast<UMaterial>(Material))
				{
					WriteMaterial(Base);
				}
				else if (UMaterialInstance* Instance = Cast<UMaterialInstance>(Material))
				{
					WriteInstance(Instance);
				}
			}
			J.EndArray();
		}

		void WriteHeader(UObject* Object)
		{
			J.Key(TEXT("path"));
			J.String(Object->GetPathName());
			J.Key(TEXT("name"));
			J.String(Object->GetName());
			J.Key(TEXT("class"));
			J.String(Object->GetClass()->GetName());
		}

		void WriteMaterial(UMaterial* Material)
		{
			J.BeginObject();
			WriteHeader(Material);
			J.Key(TEXT("parent"));
			J.Null();

			J.Key(TEXT("settings"));
			J.BeginObject();
			for (const TCHAR* Name : MaterialSettings)
			{
				if (FProperty* Property = FindFProperty<FProperty>(UMaterial::StaticClass(), Name))
				{
					J.Key(Name);
					WriteValue(Property, Property->ContainerPtrToValuePtr<void>(Material));
				}
			}
			J.Key(TEXT("ShadingModels"));
			J.BeginArray(true);
			const FMaterialShadingModelField Models = Material->GetShadingModels();
			const UEnum* ModelEnum = StaticEnum<EMaterialShadingModel>();
			for (int32 Model = 0; Model < MSM_NUM; ++Model)
			{
				if (Models.HasShadingModel((EMaterialShadingModel)Model))
				{
					J.String(ModelEnum->GetNameStringByValue(Model));
				}
			}
			J.EndArray();
			J.EndObject();

			J.Key(TEXT("pins"));
			J.BeginArray();
			const UEnum* Properties = StaticEnum<EMaterialProperty>();
			for (int32 Property = 0; Property < MP_MAX; ++Property)
			{
				FExpressionInput* Input = Material->GetExpressionInputForProperty((EMaterialProperty)Property);
				if (Input && Input->Expression)
				{
					FString Name = Properties->GetNameStringByValue(Property);
					Name.RemoveFromStart(TEXT("MP_"));
					J.BeginObject();
					J.Key(TEXT("pin"));
					J.String(Name);
					WriteLink(*Input);
					J.EndObject();
				}
			}
			J.EndArray();

			J.Key(TEXT("expressions"));
			WriteExpressions(Material->GetExpressions());
			J.EndObject();
		}

		void WriteInstance(UMaterialInstance* Instance)
		{
			J.BeginObject();
			WriteHeader(Instance);
			J.Key(TEXT("parent"));
			if (Instance->Parent)
			{
				J.String(Instance->Parent->GetPathName());
			}
			else
			{
				J.Null();
			}

			J.Key(TEXT("overrides"));
			J.BeginObject();
			J.Key(TEXT("scalar"));
			J.BeginObject();
			for (const FScalarParameterValue& Value : Instance->ScalarParameterValues)
			{
				if (Value.ParameterInfo.Association == GlobalParameter)
				{
					J.Key(Value.ParameterInfo.Name.ToString());
					J.Float(Value.ParameterValue);
				}
			}
			J.EndObject();
			J.Key(TEXT("vector"));
			J.BeginObject();
			for (const FVectorParameterValue& Value : Instance->VectorParameterValues)
			{
				if (Value.ParameterInfo.Association == GlobalParameter)
				{
					J.Key(Value.ParameterInfo.Name.ToString());
					const FLinearColor& C = Value.ParameterValue;
					J.BeginArray(true);
					J.Float(C.R);
					J.Float(C.G);
					J.Float(C.B);
					J.Float(C.A);
					J.EndArray();
				}
			}
			for (const FDoubleVectorParameterValue& Value : Instance->DoubleVectorParameterValues)
			{
				if (Value.ParameterInfo.Association == GlobalParameter)
				{
					J.Key(Value.ParameterInfo.Name.ToString());
					const FVector4d& V = Value.ParameterValue;
					J.BeginArray(true);
					J.Double(V.X);
					J.Double(V.Y);
					J.Double(V.Z);
					J.Double(V.W);
					J.EndArray();
				}
			}
			J.EndObject();
			J.Key(TEXT("texture"));
			J.BeginObject();
			for (const FTextureParameterValue& Value : Instance->TextureParameterValues)
			{
				if (Value.ParameterInfo.Association == GlobalParameter)
				{
					J.Key(Value.ParameterInfo.Name.ToString());
					WriteObject(Value.ParameterValue);
				}
			}
			J.EndObject();
			J.Key(TEXT("static_switch"));
			J.BeginObject();
			const FStaticParameterSet Statics = Instance->GetStaticParameters();
			for (const FStaticSwitchParameter& Switch : Statics.StaticSwitchParameters)
			{
				if (Switch.bOverride && Switch.ParameterInfo.Association == GlobalParameter)
				{
					J.Key(Switch.ParameterInfo.Name.ToString());
					J.Bool(Switch.Value);
				}
			}
			J.EndObject();
			if (FProperty* Base = FindFProperty<FProperty>(UMaterialInstance::StaticClass(), TEXT("BasePropertyOverrides")))
			{
				J.Key(TEXT("base_property_overrides"));
				WriteValue(Base, Base->ContainerPtrToValuePtr<void>(Instance));
			}
			J.EndObject();
			J.EndObject();
		}

		void WriteFunction(UMaterialFunctionInterface* Interface)
		{
			J.BeginObject();
			WriteHeader(Interface);
			J.Key(TEXT("expressions"));
			if (UMaterialFunction* Function = Cast<UMaterialFunction>(Interface))
			{
				WriteExpressions(Function->GetExpressions());
			}
			else
			{
				Warn(FString::Printf(TEXT("%s: material function instances are not dumped"), *Interface->GetPathName()));
				J.BeginArray();
				J.EndArray();
			}
			J.EndObject();
		}

		void WriteLink(const FExpressionInput& Input)
		{
			J.Key(TEXT("from"));
			if (Input.Expression)
			{
				J.BeginArray(true);
				J.String(Input.Expression->GetName());
				J.Int(Input.OutputIndex);
				J.EndArray();
			}
			else
			{
				J.Null();
			}
			J.Key(TEXT("mask"));
			Mask(Input.Mask, Input.MaskR, Input.MaskG, Input.MaskB, Input.MaskA, J);
		}

		void WriteExpressions(TConstArrayView<TObjectPtr<UMaterialExpression>> Expressions)
		{
			J.BeginArray();
			for (UMaterialExpression* Expression : Expressions)
			{
				if (Expression)
				{
					WriteExpression(Expression);
				}
			}
			J.EndArray();
		}

		void WriteExpression(UMaterialExpression* Expression)
		{
			J.BeginObject();
			J.Key(TEXT("name"));
			J.String(Expression->GetName());
			J.Key(TEXT("class"));
			J.String(Expression->GetClass()->GetName());

			J.Key(TEXT("properties"));
			J.BeginObject();
			for (TFieldIterator<FProperty> It(Expression->GetClass()); It; ++It)
			{
				const UClass* Owner = It->GetOwnerClass();
				if (!Owner || Owner == UMaterialExpression::StaticClass() || !Owner->IsChildOf(UMaterialExpression::StaticClass()))
				{
					continue;
				}
				if (Skipped(*It))
				{
					continue;
				}
				J.Key(It->GetName());
				WriteProperty(*It, It->ContainerPtrToValuePtr<void>(Expression));
			}
			J.EndObject();

			J.Key(TEXT("inputs"));
			J.BeginArray();
			for (FExpressionInputIterator It(Expression); It; ++It)
			{
				J.BeginObject();
				J.Key(TEXT("index"));
				J.Int(It.Index);
				J.Key(TEXT("name"));
				J.String(NameText(Expression->GetInputName(It.Index)));
				WriteLink(*It.Input);
				J.EndObject();
			}
			J.EndArray();

			J.Key(TEXT("outputs"));
			J.BeginArray();
			for (const FExpressionOutput& Output : Expression->GetOutputs())
			{
				J.BeginObject();
				J.Key(TEXT("name"));
				J.String(NameText(Output.OutputName));
				J.Key(TEXT("mask"));
				Mask(Output.Mask, Output.MaskR, Output.MaskG, Output.MaskB, Output.MaskA, J);
				J.EndObject();
			}
			J.EndArray();
			J.EndObject();
		}

		/** A property's value, as an array when the property is a fixed-size C array. */
		void WriteProperty(const FProperty* Property, const void* Value)
		{
			if (Property->ArrayDim == 1)
			{
				WriteValue(Property, Value);
				return;
			}
			J.BeginArray();
			for (int32 i = 0; i < Property->ArrayDim; ++i)
			{
				WriteValue(Property, (const uint8*)Value + i * Property->GetElementSize());
			}
			J.EndArray();
		}

		void WriteValue(const FProperty* Property, const void* Value)
		{
			if (const FBoolProperty* Bool = CastField<FBoolProperty>(Property))
			{
				J.Bool(Bool->GetPropertyValue(Value));
			}
			else if (const FEnumProperty* Enum = CastField<FEnumProperty>(Property))
			{
				const int64 Raw = Enum->GetUnderlyingProperty()->GetSignedIntPropertyValue(Value);
				J.String(Enum->GetEnum()->GetNameStringByValue(Raw));
			}
			else if (const FNumericProperty* Number = CastField<FNumericProperty>(Property))
			{
				if (const UEnum* NumberEnum = Number->GetIntPropertyEnum())
				{
					J.String(NumberEnum->GetNameStringByValue(Number->GetSignedIntPropertyValue(Value)));
				}
				else if (const FFloatProperty* Float = CastField<FFloatProperty>(Property))
				{
					J.Float(Float->GetPropertyValue(Value));
				}
				else if (Number->IsFloatingPoint())
				{
					J.Double(Number->GetFloatingPointPropertyValue(Value));
				}
				else
				{
					J.Int(Number->GetSignedIntPropertyValue(Value));
				}
			}
			else if (const FStrProperty* Str = CastField<FStrProperty>(Property))
			{
				J.String(Str->GetPropertyValue(Value));
			}
			else if (const FNameProperty* Name = CastField<FNameProperty>(Property))
			{
				J.String(NameText(Name->GetPropertyValue(Value)));
			}
			else if (const FTextProperty* Text = CastField<FTextProperty>(Property))
			{
				J.String(Text->GetPropertyValue(Value).ToString());
			}
			else if (const FSoftObjectProperty* Soft = CastField<FSoftObjectProperty>(Property))
			{
				J.String(Soft->GetPropertyValue(Value).ToString());
			}
			else if (const FObjectPropertyBase* Object = CastField<FObjectPropertyBase>(Property))
			{
				WriteObject(Object->GetObjectPropertyValue(Value));
			}
			else if (const FStructProperty* Struct = CastField<FStructProperty>(Property))
			{
				WriteStruct(Struct->Struct, Value);
			}
			else if (const FArrayProperty* Array = CastField<FArrayProperty>(Property))
			{
				FScriptArrayHelper Helper(Array, Value);
				J.BeginArray(Array->Inner->IsA<FNumericProperty>() || Array->Inner->IsA<FBoolProperty>());
				for (int32 i = 0; i < Helper.Num(); ++i)
				{
					WriteValue(Array->Inner, Helper.GetRawPtr(i));
				}
				J.EndArray();
			}
			else
			{
				J.Null();
			}
		}

		void WriteStruct(const UScriptStruct* Struct, const void* Value)
		{
			bool bVector = false;
			for (const TCHAR* Name : VectorStructs)
			{
				bVector |= Struct->GetFName() == Name;
			}
			if (bVector)
			{
				J.BeginArray(true);
				for (TFieldIterator<FProperty> It(Struct); It; ++It)
				{
					WriteValue(*It, It->ContainerPtrToValuePtr<void>(Value));
				}
				J.EndArray();
				return;
			}
			J.BeginObject();
			for (TFieldIterator<FProperty> It(Struct); It; ++It)
			{
				if (!Skipped(*It))
				{
					J.Key(It->GetName());
					WriteProperty(*It, It->ContainerPtrToValuePtr<void>(Value));
				}
			}
			J.EndObject();
		}

		/** An expression by its name; any other object by its path, noting the functions and textures reached. */
		void WriteObject(UObject* Object)
		{
			if (!Object)
			{
				J.Null();
				return;
			}
			if (UMaterialExpression* Expression = Cast<UMaterialExpression>(Object))
			{
				J.String(Expression->GetName());
				return;
			}
			if (UMaterialFunctionInterface* Function = Cast<UMaterialFunctionInterface>(Object))
			{
				Functions.AddUnique(Function);
			}
			else if (UTexture* Texture = Cast<UTexture>(Object))
			{
				Textures.AddUnique(Texture);
			}
			J.String(Object->GetPathName());
		}

		void WriteTexture(UTexture* Texture)
		{
			J.BeginObject();
			WriteHeader(Texture);
			J.Key(TEXT("file"));
			J.String(WriteTextureFile(Texture));
			J.Key(TEXT("srgb"));
			J.Bool(Texture->SRGB);
			J.Key(TEXT("compression"));
			J.String(StaticEnum<TextureCompressionSettings>()->GetNameStringByValue(Texture->CompressionSettings));
			J.Key(TEXT("filter"));
			J.String(StaticEnum<TextureFilter>()->GetNameStringByValue(Texture->Filter));
			J.Key(TEXT("flip_green"));
			J.Bool(Texture->bFlipGreenChannel);
			if (UTexture2D* Texture2D = Cast<UTexture2D>(Texture))
			{
				J.Key(TEXT("address_x"));
				J.String(StaticEnum<TextureAddress>()->GetNameStringByValue(Texture2D->AddressX));
				J.Key(TEXT("address_y"));
				J.String(StaticEnum<TextureAddress>()->GetNameStringByValue(Texture2D->AddressY));
			}
			J.Key(TEXT("size"));
			J.BeginArray(true);
			J.Int(Texture->Source.GetSizeX());
			J.Int(Texture->Source.GetSizeY());
			J.EndArray();
			J.EndObject();
		}

		/** The texture's source written to a file under the textures folder, by its path relative to the document;
		 *  empty when no folder is set or the source cannot be written. */
		FString WriteTextureFile(UTexture* Texture)
		{
			if (Options.TexturesDir.IsEmpty())
			{
				return FString();
			}
			if (!Texture->IsA<UTexture2D>() || !Texture->Source.IsValid())
			{
				Warn(FString::Printf(TEXT("%s: only 2D textures with source data are written"), *Texture->GetPathName()));
				return FString();
			}
			FImage Image;
			if (!Texture->Source.GetMipImage(Image, 0))
			{
				Warn(FString::Printf(TEXT("%s: the source could not be read"), *Texture->GetPathName()));
				return FString();
			}
			const TCHAR* Extension = ERawImageFormat::IsHDR(Image.Format) ? TEXT("exr") : TEXT("png");
			const FString Base = FPaths::MakeValidFileName(Texture->GetName());
			FString File = FString::Printf(TEXT("%s.%s"), *Base, Extension);
			for (int32 n = 2; TextureFiles.Contains(File); ++n)
			{
				File = FString::Printf(TEXT("%s_%d.%s"), *Base, n, Extension);
			}
			TextureFiles.Add(File);
			const FString Path = FPaths::Combine(Options.TexturesDir, File);
			IFileManager::Get().MakeDirectory(*Options.TexturesDir, true);
			if (!FImageUtils::SaveImageByExtension(*Path, Image))
			{
				Warn(FString::Printf(TEXT("%s: could not write %s"), *Texture->GetPathName(), *Path));
				return FString();
			}
			FString Relative = Path;
			FPaths::MakePathRelativeTo(Relative, *(Options.DocumentDir / TEXT("")));
			return Relative;
		}
	};
}

FShadewireDump ShadewireDumpGraph(TConstArrayView<UMaterialInterface*> Materials, const FShadewireDumpOptions& Options)
{
	return FDumper(Options).Run(Materials);
}
