package mssql

import "strings"

// quoteIdentifier renders name as a SQL Server identifier for interpolation into
// a statement.
//
// SQL Server brackets identifiers, and escapes an embedded closing bracket by
// doubling it. The opening bracket needs no escaping: only `]` can end the
// quoting. That asymmetry is the trap -- a helper written by analogy with the
// double-quote engines doubles the wrong character and leaves `]` able to close
// the quoting early.
//
// Quoting is unconditional, including for names that came from sys.objects,
// because "the catalog produced this name" is an assumption about a database
// this tool does not own.
func quoteIdentifier(name string) string {
	return "[" + strings.ReplaceAll(name, "]", "]]") + "]"
}

// qualify renders a schema-qualified table name.
func qualify(schema, table string) string {
	if schema == "" {
		return quoteIdentifier(table)
	}

	return quoteIdentifier(schema) + "." + quoteIdentifier(table)
}
