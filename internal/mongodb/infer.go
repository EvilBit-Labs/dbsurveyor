package mongodb

import (
	"fmt"
	"sort"
	"strings"

	"go.mongodb.org/mongo-driver/v2/bson"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// maxInferenceDepth bounds how far into a nested document inference descends.
//
// A document can nest arbitrarily, and a schema document that reported every
// path of a deeply nested blob would be longer than the data. Four levels covers
// the shapes an application actually queries by; deeper structure is reported as
// the object it is.
const maxInferenceDepth = 4

// inference accumulates what a sample of documents says about a collection's
// shape.
//
// It is a plain value with no driver in it, so the part of this adapter that has
// to be right -- deciding what a heterogeneous field is, and whether it is
// nullable -- is testable without a server.
type inference struct {
	// documents is how many were sampled, which is the denominator every
	// presence ratio is taken against.
	documents int
	fields    map[string]*fieldStats
	// order preserves first-seen field order so that a field's position does not
	// depend on Go's map iteration.
	order []string
}

// fieldStats is what the sample says about one field path.
type fieldStats struct {
	// present counts the documents that carry the field at all, including the
	// ones that carry it as null.
	present int
	// nulls counts the documents that carry it explicitly as null.
	nulls int
	// types counts the documents by the BSON type they carried.
	types map[string]int
	// typeOrder preserves first-seen type order, for stable reporting when two
	// types tie.
	typeOrder []string
}

// newInference returns an empty inference.
func newInference() *inference {
	return &inference{fields: map[string]*fieldStats{}}
}

// observe folds one sampled document into the inference.
func (i *inference) observe(document bson.M) {
	i.documents++
	i.walk(document, "", 1)
}

// walk records every field of a document, descending into nested documents up to
// maxInferenceDepth.
func (i *inference) walk(document bson.M, prefix string, depth int) {
	for name, value := range document {
		path := name
		if prefix != "" {
			path = prefix + "." + name
		}

		i.record(path, value)

		if nested, isDocument := asDocument(value); isDocument && depth < maxInferenceDepth {
			i.walk(nested, path, depth+1)
		}
	}
}

// asDocument reports whether a decoded value is a nested document, and returns
// it in a single shape.
//
// The driver does not guarantee which of its document types a subdocument
// decodes into: an ordinary decode into bson.M can still yield a bson.D for a
// nested value depending on the registry in play. A type switch on bson.M alone
// silently stopped inference at the first level, which is the bug this exists
// to close -- and it looked like working code, because the parent field was
// still reported.
func asDocument(value any) (bson.M, bool) {
	switch typed := value.(type) {
	case bson.M:
		return typed, true
	case map[string]any:
		return typed, true
	case bson.D:
		flattened := make(bson.M, len(typed))
		for _, element := range typed {
			flattened[element.Key] = element.Value
		}

		return flattened, true
	default:
		return nil, false
	}
}

// record accumulates one field occurrence.
func (i *inference) record(path string, value any) {
	stats, seen := i.fields[path]
	if !seen {
		stats = &fieldStats{types: map[string]int{}}
		i.fields[path] = stats
		i.order = append(i.order, path)
	}

	stats.present++

	bsonType := bsonTypeOf(value)
	if bsonType == typeNull {
		stats.nulls++

		// A null says the field exists and says nothing about its type, so it
		// is not counted as a type observation. Counting it would make a
		// mostly-null string field infer as "null".
		return
	}

	if _, known := stats.types[bsonType]; !known {
		stats.typeOrder = append(stats.typeOrder, bsonType)
	}

	stats.types[bsonType]++
}

// columns renders the inference as schema document columns.
//
// Fields are sorted by path rather than left in first-seen order, because
// first-seen order is the order of one arbitrary document's keys and a second
// run would produce a different one. A sorted list also puts a nested field next
// to its parent.
func (i *inference) columns() []dbschema.Column {
	paths := make([]string, len(i.order))
	copy(paths, i.order)
	sort.Strings(paths)

	columns := make([]dbschema.Column, 0, len(paths))

	for position, path := range paths {
		stats := i.fields[path]

		column := dbschema.Column{
			Name:     path,
			DataType: mapDataType(stats.dominantType()),
			// A field is nullable when the sample saw a document without it, or
			// saw it holding null. A schemaless collection has no declaration to
			// read this from, so the sample is the only evidence there is -- and
			// a field that happens to be present in every sampled document is
			// reported as non-nullable, which is a claim about the sample and
			// not about the collection.
			Nullable: stats.present < i.documents || stats.nulls > 0,
			// The identifier is the one field MongoDB guarantees and indexes.
			PrimaryKey:      path == identifierField,
			AutoGenerate:    path == identifierField,
			OrdinalPosition: uint32(position) + 1,
		}

		if summary := stats.summary(i.documents); summary != "" {
			column.Comment = &summary
		}

		columns = append(columns, column)
	}

	return columns
}

// identifierField is the field MongoDB creates on every document.
const identifierField = "_id"

// dominantType is the BSON type most documents carried for the field.
//
// A tie is broken by first-seen order rather than arbitrarily, so two runs over
// the same sample agree. A field the sample only ever saw as null has no type at
// all, which is reported as such rather than guessed.
func (s *fieldStats) dominantType() string {
	best := ""
	count := 0

	for _, bsonType := range s.typeOrder {
		if s.types[bsonType] > count {
			best = bsonType
			count = s.types[bsonType]
		}
	}

	if best == "" {
		return typeNull
	}

	return best
}

// summary describes what the sample saw, as a comment on the column.
//
// The schema document has nowhere structured to put a type frequency, and the
// frequency is the whole substance of an inferred schema: a field that is a
// string in 90% of documents and a number in the rest is a data-quality problem
// an operator needs to see, and reporting only "string" would hide it. So it
// goes in the comment, which is the field a reader already looks at for "what
// else should I know about this column".
//
// A field with one observed type and no absences produces no comment: there is
// nothing to say beyond what the column already reports.
func (s *fieldStats) summary(documents int) string {
	if documents == 0 {
		return ""
	}

	if len(s.types) <= 1 && s.nulls == 0 && s.present == documents {
		return ""
	}

	// The presence line always appears and the null line sometimes does.
	const fixedParts = 2

	parts := make([]string, 0, len(s.typeOrder)+fixedParts)
	parts = append(parts, fmt.Sprintf("present in %d of %d sampled documents", s.present, documents))

	for _, bsonType := range sortedByCount(s) {
		parts = append(parts, fmt.Sprintf("%s in %d", bsonType, s.types[bsonType]))
	}

	if s.nulls > 0 {
		parts = append(parts, fmt.Sprintf("null in %d", s.nulls))
	}

	return "inferred: " + strings.Join(parts, "; ")
}

// sortedByCount orders the observed types most-frequent first, breaking a tie by
// first-seen order so the output is stable.
func sortedByCount(s *fieldStats) []string {
	ordered := make([]string, len(s.typeOrder))
	copy(ordered, s.typeOrder)

	sort.SliceStable(ordered, func(a, b int) bool {
		return s.types[ordered[a]] > s.types[ordered[b]]
	})

	return ordered
}
