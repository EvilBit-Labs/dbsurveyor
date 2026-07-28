package mssql

import (
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// columnType is the parts of a sys.columns row that describe a column's type.
//
// MaxLength is the byte length sys.columns reports, which is not the character
// length for the Unicode types; see characterLength.
type columnType struct {
	Name      string
	MaxLength int64
	Precision uint8
	Scale     uint8
}

// mapDataType converts a SQL Server column type to the engine-independent type.
func mapDataType(column columnType) dbschema.UnifiedDataType {
	name := strings.ToLower(strings.TrimSpace(column.Name))

	if bits, ok := integerWidths[name]; ok {
		// SQL Server has exactly one unsigned integer type, and it is the
		// one-byte TINYINT. Every other integer type is signed.
		return dbschema.IntegerType(bits, name != "tinyint")
	}

	if mapped, ok := mapSimpleType(name, column); ok {
		return mapped
	}

	return dbschema.CustomType(name)
}

// mapSimpleType handles the types whose mapping does not depend on width.
func mapSimpleType(name string, column columnType) (dbschema.UnifiedDataType, bool) {
	switch name {
	case "char", "varchar", "text", "nchar", "nvarchar", "ntext", "sysname", "xml":
		return dbschema.StringType(characterLength(name, column.MaxLength)), true
	case "binary", "varbinary", "image":
		return dbschema.BinaryType(characterLength(name, column.MaxLength)), true
	case "decimal", "numeric", "money", "smallmoney", "float", "real":
		return dbschema.FloatType(&column.Precision), true
	case "bit":
		return dbschema.BooleanType(), true
	case "date":
		return dbschema.DateType(), true
	case "datetime", "datetime2", "smalldatetime":
		return dbschema.DateTimeType(false), true
	case "datetimeoffset":
		return dbschema.DateTimeType(true), true
	case "time":
		return dbschema.TimeType(false), true
	case "uniqueidentifier":
		return dbschema.UUIDType(), true
	default:
		return dbschema.UnifiedDataType{}, false
	}
}

// integerWidths is the bit width of each SQL Server integer type.
var integerWidths = map[string]uint8{
	"tinyint":  8,
	"smallint": 16,
	"int":      32,
	"bigint":   64,
}

// characterLength converts the byte length sys.columns reports into the declared
// length an operator wrote.
//
// sys.columns.max_length is in bytes, so an NVARCHAR(50) reports 100. Reporting
// that verbatim would say the column holds twice what it does. A max_length of
// -1 marks the MAX types, which have no declared limit at all.
func characterLength(name string, maxLength int64) *uint32 {
	if maxLength <= 0 || maxLength > int64(^uint32(0)) {
		return nil
	}

	if wideTypes[name] {
		maxLength /= 2
	}

	length := uint32(maxLength)

	return &length
}

// wideTypes store two bytes per character, so their byte length is twice their
// declared length.
var wideTypes = map[string]bool{
	"nchar":    true,
	"nvarchar": true,
	"ntext":    true,
	"sysname":  true,
}
