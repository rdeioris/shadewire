// USD export of materials through Shadewire. Unreal's USD level export exports each material it bakes as a USD
// layer, with an export task that names no exporter: the engine picks the exporter registered for the material's
// most derived class. These exporters are registered for UMaterial and UMaterialInstance, which Unreal's own
// (registered for UMaterialInterface) is a base of, so the level export comes here. Each writes the material as a
// Shadewire MaterialX document beside the layer and a layer whose material references it; Unreal binds that layer
// to the meshes as it binds its baked ones.

#pragma once

#include "Exporters/Exporter.h"

#include "ShadewireMaterialExporterUsd.generated.h"

UCLASS()
class UShadewireMaterialExporterUsd : public UExporter
{
	GENERATED_BODY()

public:
	UShadewireMaterialExporterUsd();

	virtual bool ExportBinary(UObject* Object, const TCHAR* Type, FArchive& Ar, FFeedbackContext* Warn, int32 FileIndex = 0, uint32 PortFlags = 0) override;
};

UCLASS()
class UShadewireMaterialInstanceExporterUsd : public UShadewireMaterialExporterUsd
{
	GENERATED_BODY()

public:
	UShadewireMaterialInstanceExporterUsd();
};
