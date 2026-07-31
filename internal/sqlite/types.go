package sqlite

import (
	"strconv"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// mapDataType converts a SQLite declared type to the engine-independent type.
//
// SQLite does not have column types in the sense the other engines do. A column
// carries whatever type name the CREATE TABLE statement wrote -- including one
// the engine has never heard of -- and the engine derives a storage affinity
// from that text by the substring rules in the SQLite documentation. So the
// mapping here is two-stage: recognize the type names that carry more meaning
// than their affinity does, then fall back to the affinity rules.
//
// The order matters. "DATETIME" has no affinity substring at all and would end
// up NUMERIC, and "BOOLEAN" would too, so the named cases have to run first.
func mapDataType(declared string) dbschema.UnifiedDataType {
	split := splitDeclaredType(declared)

	if mapped, ok := mapNamedType(split.name); ok {
		return mapped
	}

	return mapByAffinity(split.name, split.length)
}

// mapNamedType recognizes declared types whose name says more than their storage
// affinity does.
func mapNamedType(name string) (dbschema.UnifiedDataType, bool) {
	switch {
	case name == "boolean" || name == "bool":
		return dbschema.BooleanType(), true
	case strings.Contains(name, "timestamp") || strings.Contains(name, "datetime"):
		// SQLite stores no timezone with either spelling.
		return dbschema.DateTimeType(false), true
	case name == "date":
		return dbschema.DateType(), true
	case name == "time":
		return dbschema.TimeType(false), true
	case name == "json" || name == "jsonb":
		return dbschema.JSONType(), true
	case name == "uuid" || name == "guid":
		return dbschema.UUIDType(), true
	default:
		return dbschema.UnifiedDataType{}, false
	}
}

// sqliteIntegerBits is the width of every SQLite integer on disk. The declared
// name may say otherwise -- SMALLINT and TINYINT are accepted spellings -- but
// the storage is the same.
const sqliteIntegerBits uint8 = 64

// mapByAffinity applies the SQLite storage-affinity substring rules.
func mapByAffinity(name string, length *uint32) dbschema.UnifiedDataType {
	switch {
	case strings.Contains(name, "int"):
		// Every SQLite integer is a signed 64-bit value on disk, whatever width
		// the declared name claims.
		return dbschema.IntegerType(sqliteIntegerBits, true)
	case strings.Contains(name, "char"), strings.Contains(name, "clob"), strings.Contains(name, "text"):
		return dbschema.StringType(length)
	case name == "", strings.Contains(name, "blob"):
		return dbschema.BinaryType(length)
	case strings.Contains(name, "real"), strings.Contains(name, "floa"), strings.Contains(name, "doub"):
		return dbschema.FloatType(nil)
	default:
		// NUMERIC affinity. The declared name is kept rather than flattened to a
		// float, because a DECIMAL(10,2) that reports as "float" has lost the
		// only thing a reader wanted to know about it.
		return dbschema.CustomType(name)
	}
}

// declaredType is a declared type name separated from its length specifier.
type declaredType struct {
	name   string
	length *uint32
}

// splitDeclaredType lowercases a declared type and separates a leading length or
// precision, so that "VARCHAR(255)" yields "varchar" and 255.
//
// A two-part specifier such as DECIMAL(10,2) yields no length: the first number
// is a precision, not a byte or character limit, and reporting it as a maximum
// length would be wrong rather than merely imprecise.
func splitDeclaredType(declared string) declaredType {
	name := strings.ToLower(strings.TrimSpace(declared))

	open := strings.IndexByte(name, '(')
	if open < 0 {
		return declaredType{name: name}
	}

	inside, _, closed := strings.Cut(name[open+1:], ")")
	name = strings.TrimSpace(name[:open])

	if !closed || strings.Contains(inside, ",") {
		return declaredType{name: name}
	}

	size, err := strconv.ParseUint(strings.TrimSpace(inside), 10, 32)
	if err != nil {
		return declaredType{name: name}
	}

	length := uint32(size)

	return declaredType{name: name, length: &length}
}
