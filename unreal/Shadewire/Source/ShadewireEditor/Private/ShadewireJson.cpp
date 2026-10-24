#include "ShadewireJson.h"

namespace
{
	/** The shortest `%.Ng` form of `Value` that reads back as the same number, between `Min` and `Max` digits. */
	template <typename T>
	FString Shortest(T Value, int32 Min, int32 Max)
	{
		if (!FMath::IsFinite(Value))
		{
			return TEXT("0");
		}
		for (int32 Digits = Min; Digits < Max; ++Digits)
		{
			const FString Text = FString::Printf(TEXT("%.*g"), Digits, (double)Value);
			if ((T)FCString::Atod(*Text) == Value)
			{
				return Text;
			}
		}
		return FString::Printf(TEXT("%.*g"), Max, (double)Value);
	}
}

void FShadewireJson::NewLine()
{
	Out += TEXT("\n");
	for (int32 i = 0; i < Levels.Num(); ++i)
	{
		Out += TEXT("  ");
	}
}

void FShadewireJson::BeforeValue()
{
	if (bAfterKey)
	{
		bAfterKey = false;
		return;
	}
	if (Levels.Num() == 0)
	{
		return;
	}
	FLevel& Level = Levels.Last();
	if (!Level.bEmpty)
	{
		Out += Level.bInline ? TEXT(", ") : TEXT(",");
	}
	if (!Level.bInline)
	{
		NewLine();
	}
	Level.bEmpty = false;
}

void FShadewireJson::Raw(const FString& Text)
{
	BeforeValue();
	Out += Text;
}

void FShadewireJson::BeginObject()
{
	Raw(TEXT("{"));
	Levels.Add({false, false, true});
}

void FShadewireJson::EndObject()
{
	const FLevel Level = Levels.Pop();
	if (!Level.bEmpty)
	{
		NewLine();
	}
	Out += TEXT("}");
}

void FShadewireJson::BeginArray(bool bInline)
{
	Raw(TEXT("["));
	Levels.Add({true, bInline, true});
}

void FShadewireJson::EndArray()
{
	const FLevel Level = Levels.Pop();
	if (!Level.bEmpty && !Level.bInline)
	{
		NewLine();
	}
	Out += TEXT("]");
}

void FShadewireJson::Key(const FString& Name)
{
	String(Name);
	Out += TEXT(": ");
	bAfterKey = true;
}

void FShadewireJson::String(const FString& Value)
{
	FString Escaped;
	Escaped.Reserve(Value.Len() + 2);
	Escaped += TEXT("\"");
	for (const TCHAR C : Value)
	{
		switch (C)
		{
		case TEXT('"'): Escaped += TEXT("\\\""); break;
		case TEXT('\\'): Escaped += TEXT("\\\\"); break;
		case TEXT('\n'): Escaped += TEXT("\\n"); break;
		case TEXT('\r'): Escaped += TEXT("\\r"); break;
		case TEXT('\t'): Escaped += TEXT("\\t"); break;
		default:
			if (C < 0x20)
			{
				Escaped += FString::Printf(TEXT("\\u%04x"), (int32)C);
			}
			else
			{
				Escaped.AppendChar(C);
			}
		}
	}
	Escaped += TEXT("\"");
	Raw(Escaped);
}

void FShadewireJson::Float(float Value)
{
	Raw(Shortest(Value, 1, 9));
}

void FShadewireJson::Double(double Value)
{
	Raw(Shortest(Value, 1, 17));
}

void FShadewireJson::Int(int64 Value)
{
	Raw(FString::Printf(TEXT("%lld"), Value));
}

void FShadewireJson::Bool(bool Value)
{
	Raw(Value ? TEXT("true") : TEXT("false"));
}

void FShadewireJson::Null()
{
	Raw(TEXT("null"));
}
