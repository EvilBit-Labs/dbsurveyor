package main

import (
	"encoding/json"
	"fmt"
	"reflect"

	"github.com/invopop/jsonschema"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// artifact is one file genschema publishes under docs/formats/.
//
// An artifact with an id is a JSON Schema reflected from root; one without is an
// example document encoded from root directly.
type artifact struct {
	filename string
	id       string
	title    string
	root     any
}

// publishedArtifacts lists every file genschema writes.
func publishedArtifacts() []artifact {
	return append(schemaArtifacts(), exampleArtifacts()...)
}

// schemaArtifacts lists the published JSON Schema files.
func schemaArtifacts() []artifact {
	return []artifact{
		{
			filename: "schema-document.schema.json",
			id:       "https://evilbitlabs.io/dbsurveyor/schema-document.schema.json",
			title:    "dbsurveyor schema document",
			root:     &dbschema.Schema{},
		},
		{
			filename: "server-document.schema.json",
			id:       "https://evilbitlabs.io/dbsurveyor/server-document.schema.json",
			title:    "dbsurveyor server document",
			root:     &dbschema.ServerSchema{},
		},
	}
}

// render produces the artifact's file contents.
//
// Output is indented and newline-terminated so a committed file reads as a
// normal source artifact in review and diffs line by line.
func (a artifact) render() ([]byte, error) {
	if a.id == "" {
		return encodeExample(a.root)
	}

	reflector := &jsonschema.Reflector{
		// Documents are read back by this implementation only, but an operator
		// may hand-edit one. Rejecting unknown properties turns a typo into a
		// validation failure instead of a silently dropped field.
		AllowAdditionalProperties: false,
		Mapper:                    enumMapper,
	}

	schema := reflector.Reflect(a.root)
	schema.ID = jsonschema.ID(a.id)
	schema.Title = a.title

	data, err := json.MarshalIndent(schema, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("encode %s: %w", a.filename, err)
	}

	return append(data, '\n'), nil
}

// enumMapper gives every string-backed enum in dbschema an explicit value list
// in the published schema.
//
// Reflection alone sees these types as plain strings. The permitted values come
// from dbschema.AllowedValues, which is built from the same maps the runtime
// decoder enforces, so the published schema and the decoder cannot disagree.
func enumMapper(t reflect.Type) *jsonschema.Schema {
	if t.Kind() != reflect.String || t.PkgPath() != dbschemaPkgPath {
		return nil
	}

	values, ok := dbschema.AllowedValues()[t.Name()]
	if !ok {
		return nil
	}

	allowed := make([]any, 0, len(values))
	for _, v := range values {
		allowed = append(allowed, v)
	}

	return &jsonschema.Schema{Type: "string", Enum: allowed}
}

// dbschemaPkgPath is the import path whose named string types enumMapper claims.
var dbschemaPkgPath = reflect.TypeOf(dbschema.PostgreSQL).PkgPath()
