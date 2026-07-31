package oracle

import (
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// columnType is the parts of an ALL_TAB_COLUMNS row that describe a column's
// type.
//
// Precision and Scale are pointers because Oracle reports both as NULL for a
// NUMBER declared without either, which is a different type from NUMBER(38,0)
// and has to stay distinguishable.
type columnType struct {
	Name      string
	Length    *uint32
	Precision *uint8
	Scale     *int16
}

// mapDataType converts an Oracle column type to the engine-independent type.
//
// The interesting case is NUMBER, which is one type covering every numeric shape
// Oracle has. NUMBER(p,0) is an integer, NUMBER(p,s) with a scale is a fixed
// -point decimal, and a bare NUMBER is a floating value of unspecified
// precision. Reporting all three as "float" would lose the distinction an
// operator most wants from a numeric column.
func mapDataType(column columnType) dbschema.UnifiedDataType {
	name := strings.ToUpper(strings.TrimSpace(column.Name))

	// TIMESTAMP types carry their fractional precision in the name, as
	// "TIMESTAMP(6) WITH TIME ZONE", so they are matched by prefix.
	if strings.HasPrefix(name, "TIMESTAMP") {
		return dbschema.DateTimeType(strings.Contains(name, "TIME ZONE"))
	}

	if strings.HasPrefix(name, "INTERVAL") {
		return dbschema.CustomType(name)
	}

	if name == numberType {
		return mapNumber(column)
	}

	if mapped, ok := mapSimpleType(name, column); ok {
		return mapped
	}

	return dbschema.CustomType(name)
}

// numberType is Oracle's single numeric type.
const numberType = "NUMBER"

// mapNumber resolves the three shapes a NUMBER column can take.
func mapNumber(column columnType) dbschema.UnifiedDataType {
	if column.Scale != nil && *column.Scale == 0 {
		return dbschema.IntegerType(integerBits(column.Precision), true)
	}

	return dbschema.FloatType(column.Precision)
}

// integerBits picks the narrowest standard width that holds a NUMBER of the
// given decimal precision.
//
// Oracle's NUMBER is a decimal type with no bit width at all, so any mapping to
// one is a choice. The choice here is the smallest width that cannot lose a
// value: a NUMBER(9,0) holds at most 999,999,999, which fits in 32 bits, and a
// NUMBER(10,0) does not. A precision Oracle did not report means the column can
// hold up to 38 digits, which no integer width covers, so 64 is reported as the
// widest thing that is still an integer.
func integerBits(precision *uint8) uint8 {
	if precision == nil {
		return 64
	}

	switch {
	case *precision <= 4:
		return 16
	case *precision <= 9:
		return 32
	default:
		return 64
	}
}

// mapSimpleType handles the types whose mapping does not depend on width.
func mapSimpleType(name string, column columnType) (dbschema.UnifiedDataType, bool) {
	switch name {
	case "VARCHAR2", "NVARCHAR2", "CHAR", "NCHAR", "CLOB", "NCLOB", "LONG", "ROWID", "UROWID":
		return dbschema.StringType(column.Length), true
	case "BLOB", "RAW", "LONG RAW", "BFILE":
		return dbschema.BinaryType(column.Length), true
	case "BINARY_FLOAT", "BINARY_DOUBLE", "FLOAT":
		return dbschema.FloatType(column.Precision), true
	case "DATE":
		// An Oracle DATE carries a time component, unlike the DATE of every
		// other engine here, so it maps to a timestamp rather than to a date.
		return dbschema.DateTimeType(false), true
	case "JSON":
		return dbschema.JSONType(), true
	case "BOOLEAN":
		// Only 23ai and later have a SQL boolean; earlier servers never report
		// this and the case costs nothing.
		return dbschema.BooleanType(), true
	default:
		return dbschema.UnifiedDataType{}, false
	}
}
