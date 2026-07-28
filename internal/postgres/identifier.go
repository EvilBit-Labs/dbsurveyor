package postgres

import "strings"

// quoteIdentifier renders name as a PostgreSQL identifier for interpolation into
// a statement.
//
// SQL has no way to bind an identifier, so a table or column name that reaches a
// SELECT is interpolated rather than parameterized. Doubling the quote character
// is what keeps a name containing one from closing the quoting early and turning
// the remainder of the name into syntax.
//
// Quoting is unconditional, including for names that need no quoting and for
// ones that came from pg_catalog. An unquoted identifier is also case-folded to
// lower case by the parser, so a table created as "Orders" would not be found
// without the quotes at all.
func quoteIdentifier(name string) string {
	return `"` + strings.ReplaceAll(name, `"`, `""`) + `"`
}

// qualify renders a schema-qualified table name.
func qualify(schema, table string) string {
	if schema == "" {
		return quoteIdentifier(table)
	}

	return quoteIdentifier(schema) + "." + quoteIdentifier(table)
}
