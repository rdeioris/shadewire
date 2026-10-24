// A small streaming JSON writer: two-space indentation, short arrays on one line, floats in their shortest
// round-trip form, so dumps diff cleanly.

#pragma once

#include "CoreMinimal.h"

class FShadewireJson
{
public:
	void BeginObject();
	void EndObject();
	/** `bInline` writes the array on one line: for short lists of numbers. */
	void BeginArray(bool bInline = false);
	void EndArray();
	void Key(const FString& Name);

	void String(const FString& Value);
	void Float(float Value);
	void Double(double Value);
	void Int(int64 Value);
	void Bool(bool Value);
	void Null();

	const FString& Text() const { return Out; }

private:
	struct FLevel
	{
		bool bArray = false;
		bool bInline = false;
		bool bEmpty = true;
	};

	void BeforeValue();
	void NewLine();
	void Raw(const FString& Text);

	FString Out;
	TArray<FLevel> Levels;
	bool bAfterKey = false;
};
