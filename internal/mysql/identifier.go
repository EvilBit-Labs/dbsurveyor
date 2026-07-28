package mysql

import "strings"

// quoteIdentifier renders name as a MySQL identifier for interpolation into a
// statement.
//
// MySQL quotes identifiers with backticks, and escapes an embedded backtick by
// doubling it. That is a different character and a different rule from every
// other engine in this tree, which is exactly why each adapter carries its own
// helper instead of sharing one: a shared "quote an identifier" function would
// have to take the engine as an argument, and the argument is what gets passed
// wrong.
//
// It is applied to every identifier, including ones that came from
// INFORMATION_SCHEMA, because "the catalog produced this name" is an assumption
// about a database this tool does not own.
func quoteIdentifier(name string) string {
	return "`" + strings.ReplaceAll(name, "`", "``") + "`"
}

// qualify renders a database-qualified table name.
func qualify(schema, table string) string {
	if schema == "" {
		return quoteIdentifier(table)
	}

	return quoteIdentifier(schema) + "." + quoteIdentifier(table)
}
