package sqlite

import "strings"

// quoteIdentifier renders name as a SQLite identifier for interpolation into a
// statement.
//
// SQL has no way to bind an identifier, so a table or column name that reaches a
// SELECT is interpolated rather than parameterized. Doubling the quote character
// is what keeps a name containing one from closing the quoting early and turning
// the remainder of the name into syntax. It is applied to every identifier,
// including ones that came from the engine's own catalog, because "the catalog
// produced this name" is an assumption about a database this tool does not own.
//
// This is the only identifier-quoting context in this package. The metadata
// queries reach PRAGMA data through the pragma_* table-valued functions, which
// take the table name as a bound parameter, so the single-quote escaping a
// literal `PRAGMA table_info('name')` would need never arises. See collect.go.
func quoteIdentifier(name string) string {
	return `"` + strings.ReplaceAll(name, `"`, `""`) + `"`
}

// qualify renders a schema-qualified name. SQLite's only namespaces are attached
// databases, so schema is empty in practice; the parameter exists so a caller
// that has one does not have to special-case this engine.
func qualify(schema, table string) string {
	if schema == "" {
		return quoteIdentifier(table)
	}

	return quoteIdentifier(schema) + "." + quoteIdentifier(table)
}
