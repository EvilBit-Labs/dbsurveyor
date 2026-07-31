package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"
)

// formatsDir is docs/formats/ relative to this package.
const formatsDir = "../../docs/formats"

// TestPublishedArtifactsAreCurrent is the mechanism behind U3's verification
// that the published schema and the Go types agree: rather than reviewing the
// committed JSON against the structs, it regenerates every artifact and fails on
// any difference. Adding or renaming a field without running `just gen-schema`
// fails here.
func TestPublishedArtifactsAreCurrent(t *testing.T) {
	for _, artifact := range publishedArtifacts() {
		t.Run(artifact.filename, func(t *testing.T) {
			want, err := artifact.render()
			if err != nil {
				t.Fatalf("render: %v", err)
			}

			path := filepath.Join(formatsDir, artifact.filename)

			got, err := os.ReadFile(path)
			if err != nil {
				t.Fatalf("read committed artifact: %v", err)
			}

			if !bytes.Equal(normalizeNewlines(got), normalizeNewlines(want)) {
				t.Errorf("%s is stale; run `just gen-schema`", path)
			}
		})
	}
}

// TestExamplesValidateAgainstPublishedSchema checks the published examples
// against the published schema. It is not redundant with the staleness test
// above: that one proves each file matches its generator, this one proves the
// two generators agree, which is where an incorrect enum mapping or a wrong
// required-field set would surface.
func TestExamplesValidateAgainstPublishedSchema(t *testing.T) {
	cases := map[string]string{
		"examples/minimal-schema.json":  "schema-document.schema.json",
		"examples/full-schema.json":     "schema-document.schema.json",
		"examples/server-document.json": "server-document.schema.json",
	}

	for example, schemaFile := range cases {
		t.Run(example, func(t *testing.T) {
			compiled := compileSchema(t, schemaFile)

			instance := loadJSON(t, filepath.Join(formatsDir, filepath.FromSlash(example)))
			if err := compiled.Validate(instance); err != nil {
				t.Errorf("%s does not validate against %s:\n%v", example, schemaFile, err)
			}
		})
	}
}

// TestSchemaRejectsUnknownProperty proves the published schema is closed rather
// than permissive, so a hand-edited document with a typo fails validation
// instead of silently losing the field.
func TestSchemaRejectsUnknownProperty(t *testing.T) {
	compiled := compileSchema(t, "schema-document.schema.json")

	instance, ok := loadJSON(t, filepath.Join(formatsDir, "examples", "minimal-schema.json")).(map[string]any)
	if !ok {
		t.Fatal("example is not a JSON object")
	}

	instance["formt_version"] = "1.0"

	if err := compiled.Validate(instance); err == nil {
		t.Error("expected validation to reject an unknown property")
	}
}

// TestSchemaRejectsUnknownEnumValue proves the enum mapping reached the
// published schema, not just the Go decoder.
func TestSchemaRejectsUnknownEnumValue(t *testing.T) {
	compiled := compileSchema(t, "schema-document.schema.json")

	instance, ok := loadJSON(t, filepath.Join(formatsDir, "examples", "minimal-schema.json")).(map[string]any)
	if !ok {
		t.Fatal("example is not a JSON object")
	}

	info, ok := instance["database_info"].(map[string]any)
	if !ok {
		t.Fatal("database_info is not a JSON object")
	}

	info["type"] = "cockroachdb"

	if err := compiled.Validate(instance); err == nil {
		t.Error("expected validation to reject an unrecognized database type")
	}
}

// compileSchema compiles a published schema file for validation.
func compileSchema(t *testing.T, name string) *jsonschema.Schema {
	t.Helper()

	compiler := jsonschema.NewCompiler()
	if err := compiler.AddResource(name, loadJSON(t, filepath.Join(formatsDir, name))); err != nil {
		t.Fatalf("add schema resource: %v", err)
	}

	compiled, err := compiler.Compile(name)
	if err != nil {
		t.Fatalf("compile schema: %v", err)
	}

	return compiled
}

// loadJSON reads and decodes a JSON file for the validator.
func loadJSON(t *testing.T, path string) any {
	t.Helper()

	file, err := os.Open(path)
	if err != nil {
		t.Fatalf("open %s: %v", path, err)
	}
	defer func() { _ = file.Close() }()

	doc, err := jsonschema.UnmarshalJSON(file)
	if err != nil {
		t.Fatalf("decode %s: %v", path, err)
	}

	return doc
}

// normalizeNewlines strips carriage returns so the comparison holds on Windows
// checkouts configured with autocrlf.
func normalizeNewlines(b []byte) []byte {
	return bytes.ReplaceAll(b, []byte("\r\n"), []byte("\n"))
}
