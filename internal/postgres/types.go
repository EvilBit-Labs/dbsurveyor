package postgres

import (
	"strconv"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// mapDataType converts a PostgreSQL type to the engine-independent type.
//
// typeName is the internal name from pg_type ("int4", "_text"); fullType is
// what format_type produced ("integer", "character varying(64)", "text[]").
// Both are needed: the internal name is unambiguous and stable, and the full
// form is where a declared length or an array marker appears.
func mapDataType(typeName, fullType string) dbschema.UnifiedDataType {
	name := strings.ToLower(strings.TrimSpace(typeName))

	// An array type is the element type with a leading underscore. The document
	// nests the element rather than flattening, so a text[] stays legible as
	// array<string> instead of collapsing to "custom".
	if element, isArray := strings.CutPrefix(name, "_"); isArray {
		return dbschema.ArrayType(mapDataType(element, strings.TrimSuffix(fullType, "[]")))
	}

	if bits, ok := integerWidths[name]; ok {
		// Every PostgreSQL integer is signed. There is no unsigned integer type.
		return dbschema.IntegerType(bits, true)
	}

	if mapped, ok := mapSimpleType(name, fullType); ok {
		return mapped
	}

	return dbschema.CustomType(name)
}

// mapSimpleType handles the types whose mapping does not depend on width.
func mapSimpleType(name, fullType string) (dbschema.UnifiedDataType, bool) {
	switch name {
	case "bpchar", "varchar", "text", "name", "citext":
		return dbschema.StringType(declaredLength(fullType)), true
	case "bytea":
		return dbschema.BinaryType(nil), true
	case "float4", "float8", "numeric":
		return dbschema.FloatType(declaredPrecision(fullType)), true
	case "bool":
		return dbschema.BooleanType(), true
	case "date":
		return dbschema.DateType(), true
	case "timestamp":
		return dbschema.DateTimeType(false), true
	case "timestamptz":
		return dbschema.DateTimeType(true), true
	case "time":
		return dbschema.TimeType(false), true
	case "timetz":
		return dbschema.TimeType(true), true
	case "json", "jsonb":
		return dbschema.JSONType(), true
	case "uuid":
		return dbschema.UUIDType(), true
	default:
		return dbschema.UnifiedDataType{}, false
	}
}

// integerWidths is the bit width of each PostgreSQL integer type.
var integerWidths = map[string]uint8{
	"int2": 16,
	"int4": 32,
	"int8": 64,
}

// declaredLength reads the length out of a formatted type such as
// "character varying(64)".
//
// A two-part specifier belongs to numeric, where the first number is a precision
// rather than a length, so it yields nothing here.
func declaredLength(fullType string) *uint32 {
	inside, ok := parenthesized(fullType)
	if !ok || strings.Contains(inside, ",") {
		return nil
	}

	size, err := strconv.ParseUint(inside, 10, 32)
	if err != nil {
		return nil
	}

	length := uint32(size)

	return &length
}

// declaredPrecision reads the precision out of a formatted type such as
// "numeric(10,2)".
func declaredPrecision(fullType string) *uint8 {
	inside, ok := parenthesized(fullType)
	if !ok {
		return nil
	}

	leading, _, _ := strings.Cut(inside, ",")

	digits, err := strconv.ParseUint(strings.TrimSpace(leading), 10, 8)
	if err != nil {
		return nil
	}

	precision := uint8(digits)

	return &precision
}

// parenthesized returns the contents of the first parenthesized group.
func parenthesized(fullType string) (string, bool) {
	open := strings.IndexByte(fullType, '(')
	if open < 0 {
		return "", false
	}

	inside, _, closed := strings.Cut(fullType[open+1:], ")")
	if !closed {
		return "", false
	}

	return strings.TrimSpace(inside), true
}
