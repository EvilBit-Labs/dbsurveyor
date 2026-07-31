package oracle

import "strings"

// quoteIdentifier renders name as an Oracle identifier for interpolation into a
// statement.
//
// Oracle quotes with double quotes and escapes an embedded one by doubling it,
// the same rule as PostgreSQL and SQLite. The difference is what happens when
// the quotes are left off: Oracle folds an unquoted identifier to *upper* case,
// where PostgreSQL folds to lower. A table created as `orders` is stored as
// ORDERS and is found by "ORDERS", never by "orders".
//
// That is why the catalog queries in this package compare against the name as
// the catalog stores it, and why normalize exists.
func quoteIdentifier(name string) string {
	return `"` + strings.ReplaceAll(name, `"`, `""`) + `"`
}

// qualify renders an owner-qualified table name.
func qualify(owner, table string) string {
	if owner == "" {
		return quoteIdentifier(table)
	}

	return quoteIdentifier(owner) + "." + quoteIdentifier(table)
}

// normalize renders a name the way the catalog stores it.
//
// An identifier an operator typed unquoted was folded to upper case on the way
// in, so a lookup by the name as typed finds nothing. A name that was created
// quoted keeps whatever case it was given, and this would break it -- so
// normalize applies only when the name has no lower-case letters to lose, which
// is the same test the engine's own folding makes.
func normalize(name string) string {
	if name != strings.ToLower(name) {
		// The caller already has a name with upper-case letters in it, which
		// means either it came from the catalog or it was created quoted. Either
		// way it is already the stored form.
		return name
	}

	return strings.ToUpper(name)
}
