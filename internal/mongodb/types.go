package mongodb

import (
	"time"

	"go.mongodb.org/mongo-driver/v2/bson"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// The BSON type names this package infers. They are the names MongoDB's own
// $type operator uses, so a warning or a comment naming one is something an
// operator can paste into a query.
const (
	typeString   = "string"
	typeInt      = "int"
	typeLong     = "long"
	typeDouble   = "double"
	typeDecimal  = "decimal"
	typeBool     = "bool"
	typeDate     = "date"
	typeObjectID = "objectId"
	typeObject   = "object"
	typeArray    = "array"
	typeBinary   = "binData"
	typeNull     = "null"
	typeUnknown  = "unknown"
)

// bsonTypeOf names the BSON type of a decoded value.
//
// The driver decodes into Go types, so the BSON type has to be recovered from
// the Go one. int32 and int64 are distinguished because MongoDB does: a field
// holding both is a field two writers disagreed about, and collapsing them to
// "number" would hide exactly that.
func bsonTypeOf(value any) string {
	switch value.(type) {
	case nil:
		return typeNull
	case string:
		return typeString
	case int32:
		return typeInt
	case int64, int:
		return typeLong
	case float64, float32:
		return typeDouble
	case bool:
		return typeBool
	case time.Time, bson.DateTime:
		return typeDate
	case bson.ObjectID:
		return typeObjectID
	case bson.Decimal128:
		return typeDecimal
	case bson.Binary:
		return typeBinary
	case bson.M, bson.D:
		return typeObject
	case bson.A, []any:
		return typeArray
	default:
		return typeUnknown
	}
}

// mapDataType converts an inferred BSON type name to the engine-independent
// type.
func mapDataType(bsonType string) dbschema.UnifiedDataType {
	switch bsonType {
	case typeString:
		// A BSON string carries no declared length.
		return dbschema.StringType(nil)
	case typeInt:
		return dbschema.IntegerType(32, true)
	case typeLong:
		return dbschema.IntegerType(64, true)
	case typeDouble, typeDecimal:
		return dbschema.FloatType(nil)
	case typeBool:
		return dbschema.BooleanType()
	case typeDate:
		// A BSON date is milliseconds since the epoch in UTC, so it carries a
		// zone even though nothing in the document names one.
		return dbschema.DateTimeType(true)
	case typeBinary:
		return dbschema.BinaryType(nil)
	case typeObject:
		// A nested document is JSON in every sense the schema document means.
		return dbschema.JSONType()
	case typeArray:
		// The element type is not inferred: an array in a schemaless collection
		// routinely holds more than one kind of thing, and naming one of them
		// would be a claim the sample does not support.
		return dbschema.ArrayType(dbschema.CustomType(typeUnknown))
	case typeObjectID:
		// An ObjectID is a 12-byte identifier, not a UUID. Reporting it as one
		// would say it is interchangeable with the UUIDs of the other engines.
		return dbschema.CustomType(typeObjectID)
	default:
		return dbschema.CustomType(bsonType)
	}
}
