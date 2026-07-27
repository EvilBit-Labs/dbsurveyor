// Package dbschema defines the database schema document and every operation
// over it: quality scoring, redaction, validation, and credential scanning.
//
// The package is a leaf. It imports nothing else from this module, so adapters,
// orchestration, and the command layer may all depend on it without creating a
// cycle.
package dbschema

import (
	"encoding/json"
	"fmt"
	"sort"
)

// Format is the document discriminator written into every schema document.
// It distinguishes a dbsurveyor schema document from arbitrary JSON, and from
// documents produced by the retired Rust implementation, which carried no such
// key. See docs/formats/schema-document.md.
const Format = "dbsurveyor/schema"

// FormatVersion is the version of the schema document format implemented here.
// The Go implementation is the reference for this format; it carries no
// obligation to read documents produced by the retired Rust implementation.
const FormatVersion = "1.0"

// DatabaseType identifies the database engine a schema was collected from.
type DatabaseType string

// The database engines dbsurveyor collects from.
const (
	PostgreSQL DatabaseType = "postgresql"
	MySQL      DatabaseType = "mysql"
	SQLite     DatabaseType = "sqlite"
	MongoDB    DatabaseType = "mongodb"
	SQLServer  DatabaseType = "sqlserver"
	Oracle     DatabaseType = "oracle"
)

var databaseTypeNames = map[DatabaseType]string{
	PostgreSQL: "PostgreSQL",
	MySQL:      "MySQL",
	SQLite:     "SQLite",
	MongoDB:    "MongoDB",
	SQLServer:  "SQL Server",
	Oracle:     "Oracle",
}

// String returns the engine's display name, for example "SQL Server".
func (d DatabaseType) String() string {
	if name, ok := databaseTypeNames[d]; ok {
		return name
	}

	return string(d)
}

// Valid reports whether d is a recognized database engine.
func (d DatabaseType) Valid() bool {
	_, ok := databaseTypeNames[d]

	return ok
}

// UnmarshalJSON rejects unrecognized engine names rather than accepting them as
// an opaque string, so a corrupted or hand-edited document fails at the parse
// boundary instead of at the point of use.
func (d *DatabaseType) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "database type", d)
}

// AccessLevel records how much of a database the collecting credential could read.
type AccessLevel string

// The access levels a collection run can report.
const (
	AccessFull    AccessLevel = "full"
	AccessLimited AccessLevel = "limited"
	AccessNone    AccessLevel = "none"
)

var accessLevels = map[AccessLevel]struct{}{
	AccessFull:    {},
	AccessLimited: {},
	AccessNone:    {},
}

// Valid reports whether a is a recognized access level.
func (a AccessLevel) Valid() bool {
	_, ok := accessLevels[a]

	return ok
}

// UnmarshalJSON rejects unrecognized access levels.
func (a *AccessLevel) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "access level", a)
}

// ReferentialAction is the action a foreign key takes on the child row when the
// parent row changes. A nil action on a ForeignKey means the engine default
// applies; it is not the same as NoAction.
type ReferentialAction string

// The referential actions a foreign key can declare.
const (
	Cascade    ReferentialAction = "cascade"
	SetNull    ReferentialAction = "set_null"
	SetDefault ReferentialAction = "set_default"
	Restrict   ReferentialAction = "restrict"
	NoAction   ReferentialAction = "no_action"
)

var referentialActions = map[ReferentialAction]struct{}{
	Cascade:    {},
	SetNull:    {},
	SetDefault: {},
	Restrict:   {},
	NoAction:   {},
}

// Valid reports whether r is a recognized referential action.
func (r ReferentialAction) Valid() bool {
	_, ok := referentialActions[r]

	return ok
}

// UnmarshalJSON rejects unrecognized referential actions.
func (r *ReferentialAction) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "referential action", r)
}

// ConstraintType classifies a table constraint.
type ConstraintType string

// The constraint kinds a schema document can carry.
const (
	ConstraintPrimaryKey ConstraintType = "primary_key"
	ConstraintForeignKey ConstraintType = "foreign_key"
	ConstraintUnique     ConstraintType = "unique"
	ConstraintCheck      ConstraintType = "check"
	ConstraintNotNull    ConstraintType = "not_null"
)

var constraintTypes = map[ConstraintType]struct{}{
	ConstraintPrimaryKey: {},
	ConstraintForeignKey: {},
	ConstraintUnique:     {},
	ConstraintCheck:      {},
	ConstraintNotNull:    {},
}

// Valid reports whether c is a recognized constraint type.
func (c ConstraintType) Valid() bool {
	_, ok := constraintTypes[c]

	return ok
}

// UnmarshalJSON rejects unrecognized constraint types.
func (c *ConstraintType) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "constraint type", c)
}

// TriggerEvent is the data-modification event a trigger fires on.
type TriggerEvent string

// The events a trigger can fire on.
const (
	TriggerInsert TriggerEvent = "insert"
	TriggerUpdate TriggerEvent = "update"
	TriggerDelete TriggerEvent = "delete"
)

var triggerEvents = map[TriggerEvent]struct{}{
	TriggerInsert: {},
	TriggerUpdate: {},
	TriggerDelete: {},
}

// Valid reports whether t is a recognized trigger event.
func (t TriggerEvent) Valid() bool {
	_, ok := triggerEvents[t]

	return ok
}

// UnmarshalJSON rejects unrecognized trigger events.
func (t *TriggerEvent) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "trigger event", t)
}

// TriggerTiming is when a trigger fires relative to its event.
type TriggerTiming string

// The timings a trigger can declare.
const (
	TimingBefore    TriggerTiming = "before"
	TimingAfter     TriggerTiming = "after"
	TimingInsteadOf TriggerTiming = "instead_of"
)

var triggerTimings = map[TriggerTiming]struct{}{
	TimingBefore:    {},
	TimingAfter:     {},
	TimingInsteadOf: {},
}

// Valid reports whether t is a recognized trigger timing.
func (t TriggerTiming) Valid() bool {
	_, ok := triggerTimings[t]

	return ok
}

// UnmarshalJSON rejects unrecognized trigger timings.
func (t *TriggerTiming) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "trigger timing", t)
}

// TypeCategory classifies a user-defined type.
type TypeCategory string

// The categories a user-defined type can fall into.
const (
	CategoryEnum      TypeCategory = "enum"
	CategoryComposite TypeCategory = "composite"
	CategoryDomain    TypeCategory = "domain"
	CategoryRange     TypeCategory = "range"
)

var typeCategories = map[TypeCategory]struct{}{
	CategoryEnum:      {},
	CategoryComposite: {},
	CategoryDomain:    {},
	CategoryRange:     {},
}

// Valid reports whether t is a recognized type category.
func (t TypeCategory) Valid() bool {
	_, ok := typeCategories[t]

	return ok
}

// UnmarshalJSON rejects unrecognized type categories.
func (t *TypeCategory) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "type category", t)
}

// ParameterDirection is whether a routine parameter is read, written, or both.
type ParameterDirection string

// The directions a routine parameter can declare.
const (
	DirectionIn    ParameterDirection = "in"
	DirectionOut   ParameterDirection = "out"
	DirectionInOut ParameterDirection = "inout"
)

var parameterDirections = map[ParameterDirection]struct{}{
	DirectionIn:    {},
	DirectionOut:   {},
	DirectionInOut: {},
}

// Valid reports whether p is a recognized parameter direction.
func (p ParameterDirection) Valid() bool {
	_, ok := parameterDirections[p]

	return ok
}

// UnmarshalJSON rejects unrecognized parameter directions.
func (p *ParameterDirection) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "parameter direction", p)
}

// SortDirection is the direction an index column or ordering column is sorted in.
type SortDirection string

// The sort directions an ordered column can declare.
const (
	Ascending  SortDirection = "asc"
	Descending SortDirection = "desc"
)

var sortDirections = map[SortDirection]struct{}{
	Ascending:  {},
	Descending: {},
}

// Valid reports whether s is a recognized sort direction.
func (s SortDirection) Valid() bool {
	_, ok := sortDirections[s]

	return ok
}

// UnmarshalJSON rejects unrecognized sort directions.
func (s *SortDirection) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "sort direction", s)
}

// DataTypeKind is the discriminator of a UnifiedDataType.
type DataTypeKind string

// The engine-independent type kinds a column, parameter, or return value maps to.
const (
	TypeString   DataTypeKind = "string"
	TypeInteger  DataTypeKind = "integer"
	TypeFloat    DataTypeKind = "float"
	TypeBoolean  DataTypeKind = "boolean"
	TypeDateTime DataTypeKind = "datetime"
	TypeDate     DataTypeKind = "date"
	TypeTime     DataTypeKind = "time"
	TypeBinary   DataTypeKind = "binary"
	TypeJSON     DataTypeKind = "json"
	TypeUUID     DataTypeKind = "uuid"
	TypeArray    DataTypeKind = "array"
	TypeCustom   DataTypeKind = "custom"
)

var dataTypeKinds = map[DataTypeKind]struct{}{
	TypeString:   {},
	TypeInteger:  {},
	TypeFloat:    {},
	TypeBoolean:  {},
	TypeDateTime: {},
	TypeDate:     {},
	TypeTime:     {},
	TypeBinary:   {},
	TypeJSON:     {},
	TypeUUID:     {},
	TypeArray:    {},
	TypeCustom:   {},
}

// Valid reports whether k is a recognized data type kind.
func (k DataTypeKind) Valid() bool {
	_, ok := dataTypeKinds[k]

	return ok
}

// UnmarshalJSON rejects unrecognized data type kinds.
func (k *DataTypeKind) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "data type kind", k)
}

// UnifiedDataType is an engine-independent description of a column, parameter,
// or return type. Kind is the discriminator; the remaining fields are the
// payload for the kinds that carry one and are absent from the marshalled
// document otherwise.
//
// This is the Go rendering of a tagged union. Go has no sum type, so the
// invariant "only the fields belonging to Kind are set" is enforced by the
// constructors below and by Validate rather than by the type system. Construct
// values through the constructors; the zero value is not a valid type.
type UnifiedDataType struct {
	Kind DataTypeKind `json:"kind"`
	// MaxLength is the declared length limit for TypeString and TypeBinary.
	// Nil means the engine reported no limit.
	MaxLength *uint32 `json:"max_length,omitempty"`
	// Bits is the width of a TypeInteger, for example 32 or 64.
	Bits *uint8 `json:"bits,omitempty"`
	// Signed is whether a TypeInteger is signed.
	Signed *bool `json:"signed,omitempty"`
	// Precision is the declared precision of a TypeFloat, if the engine reports one.
	Precision *uint8 `json:"precision,omitempty"`
	// WithTimezone applies to TypeDateTime and TypeTime.
	WithTimezone *bool `json:"with_timezone,omitempty"`
	// ElementType is the element of a TypeArray.
	ElementType *UnifiedDataType `json:"element_type,omitempty"`
	// TypeName is the engine-specific name of a TypeCustom.
	TypeName string `json:"type_name,omitempty"`
}

// StringType builds a string type. Pass nil for maxLength when the engine
// reports no declared limit.
func StringType(maxLength *uint32) UnifiedDataType {
	return UnifiedDataType{Kind: TypeString, MaxLength: maxLength}
}

// IntegerType builds an integer type of the given bit width and signedness.
func IntegerType(bits uint8, signed bool) UnifiedDataType {
	return UnifiedDataType{Kind: TypeInteger, Bits: &bits, Signed: &signed}
}

// FloatType builds a floating-point type. Pass nil for precision when the
// engine reports none.
func FloatType(precision *uint8) UnifiedDataType {
	return UnifiedDataType{Kind: TypeFloat, Precision: precision}
}

// BooleanType builds a boolean type.
func BooleanType() UnifiedDataType {
	return UnifiedDataType{Kind: TypeBoolean}
}

// DateTimeType builds a timestamp type, with or without a timezone.
func DateTimeType(withTimezone bool) UnifiedDataType {
	return UnifiedDataType{Kind: TypeDateTime, WithTimezone: &withTimezone}
}

// DateType builds a date-only type.
func DateType() UnifiedDataType {
	return UnifiedDataType{Kind: TypeDate}
}

// TimeType builds a time-of-day type, with or without a timezone.
func TimeType(withTimezone bool) UnifiedDataType {
	return UnifiedDataType{Kind: TypeTime, WithTimezone: &withTimezone}
}

// BinaryType builds a binary type. Pass nil for maxLength when the engine
// reports no declared limit.
func BinaryType(maxLength *uint32) UnifiedDataType {
	return UnifiedDataType{Kind: TypeBinary, MaxLength: maxLength}
}

// JSONType builds a JSON or JSONB type.
func JSONType() UnifiedDataType {
	return UnifiedDataType{Kind: TypeJSON}
}

// UUIDType builds a UUID type.
func UUIDType() UnifiedDataType {
	return UnifiedDataType{Kind: TypeUUID}
}

// ArrayType builds an array type over the given element type.
func ArrayType(element UnifiedDataType) UnifiedDataType {
	return UnifiedDataType{Kind: TypeArray, ElementType: &element}
}

// CustomType builds an engine-specific type identified by name.
func CustomType(name string) UnifiedDataType {
	return UnifiedDataType{Kind: TypeCustom, TypeName: name}
}

// String renders the type in a short, human-readable form suitable for a report
// cell, for example "varchar(255)" becomes "string(255)" and a nested array
// becomes "array<integer>".
func (u UnifiedDataType) String() string {
	switch u.Kind {
	case TypeString, TypeBinary:
		if u.MaxLength != nil {
			return fmt.Sprintf("%s(%d)", u.Kind, *u.MaxLength)
		}
	case TypeInteger:
		if u.Bits != nil {
			signed := "u"
			if u.Signed == nil || *u.Signed {
				signed = "i"
			}

			return fmt.Sprintf("%s%d", signed, *u.Bits)
		}
	case TypeFloat:
		if u.Precision != nil {
			return fmt.Sprintf("float(%d)", *u.Precision)
		}
	case TypeDateTime, TypeTime:
		if u.WithTimezone != nil && *u.WithTimezone {
			return string(u.Kind) + "tz"
		}
	case TypeArray:
		if u.ElementType != nil {
			return "array<" + u.ElementType.String() + ">"
		}
	case TypeCustom:
		if u.TypeName != "" {
			return u.TypeName
		}
	case TypeBoolean, TypeDate, TypeJSON, TypeUUID:
	}

	return string(u.Kind)
}

// validator is implemented by every string-backed enum in this package. It lets
// unmarshalEnum validate any of them without a per-type switch.
type validator interface {
	Valid() bool
}

// unmarshalEnum decodes a JSON string into a string-backed enum and rejects any
// value the type does not recognize.
//
// Go's default behavior for a named string type is to accept whatever the
// document contains, which turns a typo or a document from a future format
// version into a silently wrong value that only surfaces much later. Failing at
// the parse boundary keeps the error next to its cause.
func unmarshalEnum[T ~string, P interface {
	*T
	validator
}](data []byte, kind string, out P) error {
	var raw string
	if err := json.Unmarshal(data, &raw); err != nil {
		return fmt.Errorf("decode %s: %w", kind, err)
	}

	*out = T(raw)
	if !out.Valid() {
		return fmt.Errorf("unknown %s %q", kind, raw)
	}

	return nil
}

// AllowedValues maps each string-backed enum type in this package to its
// permitted values, keyed by Go type name and sorted for stable output.
//
// It exists so the published JSON Schema is generated from the same constants
// the code enforces rather than hand-maintained beside them. Adding an enum
// type without registering it here makes TestAllowedValuesCoversEveryEnum fail.
func AllowedValues() map[string][]string {
	return map[string][]string{
		"DatabaseType":       keysOf(databaseTypeNames),
		"AccessLevel":        keysOf(accessLevels),
		"ReferentialAction":  keysOf(referentialActions),
		"ConstraintType":     keysOf(constraintTypes),
		"TriggerEvent":       keysOf(triggerEvents),
		"TriggerTiming":      keysOf(triggerTimings),
		"TypeCategory":       keysOf(typeCategories),
		"ParameterDirection": keysOf(parameterDirections),
		"SortDirection":      keysOf(sortDirections),
		"DataTypeKind":       keysOf(dataTypeKinds),
		"SampleState":        keysOf(sampleStates),
		"SamplingKind":       keysOf(samplingKinds),
		"OrderingKind":       keysOf(orderingKinds),
		"CollectionState":    keysOf(collectionStates),
		"CollectionModeKind": keysOf(collectionModeKinds),
	}
}

// keysOf returns the keys of an enum-membership map as a sorted string slice.
func keysOf[T ~string, V any](m map[T]V) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, string(k))
	}

	sort.Strings(out)

	return out
}
