package mysql

import (
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// columnType is the parts of an INFORMATION_SCHEMA.COLUMNS row that describe a
// column's type.
//
// DataType is the bare type name ("varchar"); FullType is the complete
// declaration ("varchar(64)", "int unsigned"). Both are needed: the bare name is
// what the mapping switches on, and signedness appears only in the full form.
type columnType struct {
	DataType  string
	FullType  string
	MaxLength *uint32
	Precision *uint8
}

// mapDataType converts a MySQL column type to the engine-independent type.
func mapDataType(column columnType) dbschema.UnifiedDataType {
	name := strings.ToLower(strings.TrimSpace(column.DataType))

	if isBooleanDeclaration(column) {
		return dbschema.BooleanType()
	}

	if bits, ok := integerWidths[name]; ok {
		// Signedness lives in COLUMN_TYPE, never in DATA_TYPE. A column declared
		// INT UNSIGNED reports DATA_TYPE "int" exactly as a signed one does.
		signed := !strings.Contains(strings.ToLower(column.FullType), "unsigned")

		return dbschema.IntegerType(bits, signed)
	}

	if mapped, ok := mapSimpleType(name, column); ok {
		return mapped
	}

	return dbschema.CustomType(name)
}

// mapSimpleType handles the types whose mapping does not depend on width.
func mapSimpleType(name string, column columnType) (dbschema.UnifiedDataType, bool) {
	switch name {
	case "char", "varchar", "tinytext", "text", "mediumtext", "longtext", "enum", "set":
		return dbschema.StringType(column.MaxLength), true
	case "binary", "varbinary", "tinyblob", "blob", "mediumblob", "longblob":
		return dbschema.BinaryType(column.MaxLength), true
	case "decimal", "numeric", "float", "double":
		return dbschema.FloatType(column.Precision), true
	case "date":
		return dbschema.DateType(), true
	case "datetime":
		// DATETIME stores a wall-clock value with no zone.
		return dbschema.DateTimeType(false), true
	case "timestamp":
		// TIMESTAMP is converted to UTC on write and back on read, so it does
		// carry a zone even though the column does not name one.
		return dbschema.DateTimeType(true), true
	case "time":
		return dbschema.TimeType(false), true
	case "json":
		return dbschema.JSONType(), true
	default:
		return dbschema.UnifiedDataType{}, false
	}
}

// integerWidths is the declared bit width of each MySQL integer type. YEAR is
// here because it is stored and compared as a number, not as a date.
var integerWidths = map[string]uint8{
	"tinyint":   8,
	"smallint":  16,
	"mediumint": 32,
	"int":       32,
	"integer":   32,
	"bigint":    64,
	"year":      16,
}

// isBooleanDeclaration reports whether a column was declared BOOL or BOOLEAN.
//
// MySQL has no boolean type: BOOLEAN is an alias for TINYINT(1), and
// INFORMATION_SCHEMA reports the alias only in COLUMN_TYPE. Recovering the
// declaration matters because a column of ones and zeroes reported as an 8-bit
// integer tells a reader nothing about what the values mean.
func isBooleanDeclaration(column columnType) bool {
	full := strings.ToLower(strings.TrimSpace(column.FullType))

	return full == "tinyint(1)" || strings.HasPrefix(full, "bool")
}
