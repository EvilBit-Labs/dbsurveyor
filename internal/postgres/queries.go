package postgres

import "strings"

// The five per-table metadata queries, written once with a placeholder where
// their table predicate goes.
//
// Each is used two ways: batched across every table in the survey, and scoped to
// a single table for the fallback path. Writing one template per query rather
// than two statements is what keeps the two paths producing identical documents
// -- the select list, the joins, and the ordering are literally the same text,
// so the fallback cannot drift into reporting something subtly different from
// the batch it replaced. See batch.go.
const predicatePlaceholder = "{{predicate}}"

// The two predicates a template is completed with. They are constants, not
// values from anywhere a caller controls: the schema and table names travel as
// bound parameters in both forms.
const (
	schemaPredicate = `n.nspname = ANY($1)`
	tablePredicate  = `n.nspname = $1 AND c.relname = $2`
)

// columnsTemplate reads columns from pg_attribute rather than from
// information_schema.columns.
//
// The catalog view would be more portable and is worse here: it cannot report a
// column comment without a second lookup, it does not distinguish an identity
// column from one with a nextval default, and it hides the attnum gaps that
// dropped columns leave. All three matter to the document.
const columnsTemplate = `
SELECT n.nspname,
       c.relname,
       a.attname,
       t.typname,
       format_type(a.atttypid, a.atttypmod),
       NOT a.attnotnull,
       pg_get_expr(d.adbin, d.adrelid),
       a.attidentity <> '',
       col_description(c.oid, a.attnum)
FROM pg_attribute a
JOIN pg_class c ON c.oid = a.attrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_type t ON t.oid = a.atttypid
LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
WHERE a.attnum > 0
  AND NOT a.attisdropped
  AND c.relkind IN ('r', 'p', 'v', 'm')
  AND ` + predicatePlaceholder + `
ORDER BY n.nspname, c.relname, a.attnum`

// keysTemplate reads primary-key and unique constraints.
//
// The column arrays are built by unnesting conkey and looking each attribute
// number up, ordered by its position in conkey rather than by attribute number:
// a compound key declares its columns in an order that need not match the
// table's, and that order is part of what the key means.
const keysTemplate = `
SELECT n.nspname,
       c.relname,
       con.conname,
       con.contype::text,
       ARRAY(SELECT a.attname
             FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum
             ORDER BY k.ord)
FROM pg_constraint con
JOIN pg_class c ON c.oid = con.conrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE con.contype IN ('p', 'u')
  AND ` + predicatePlaceholder + `
ORDER BY n.nspname, c.relname, con.conname`

// foreignKeysTemplate reads foreign keys with their referential actions.
const foreignKeysTemplate = `
SELECT n.nspname,
       c.relname,
       con.conname,
       ARRAY(SELECT a.attname
             FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum
             ORDER BY k.ord),
       fn.nspname,
       fc.relname,
       ARRAY(SELECT a.attname
             FROM unnest(con.confkey) WITH ORDINALITY AS k(attnum, ord)
             JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.attnum
             ORDER BY k.ord),
       con.confupdtype::text,
       con.confdeltype::text
FROM pg_constraint con
JOIN pg_class c ON c.oid = con.conrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_class fc ON fc.oid = con.confrelid
JOIN pg_namespace fn ON fn.oid = fc.relnamespace
WHERE con.contype = 'f'
  AND ` + predicatePlaceholder + `
ORDER BY n.nspname, c.relname, con.conname`

// checksTemplate reads check constraints with their clauses.
const checksTemplate = `
SELECT n.nspname,
       c.relname,
       con.conname,
       pg_get_constraintdef(con.oid),
       ARRAY(SELECT a.attname
             FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord)
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.attnum
             ORDER BY k.ord)
FROM pg_constraint con
JOIN pg_class c ON c.oid = con.conrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE con.contype = 'c'
  AND ` + predicatePlaceholder + `
ORDER BY n.nspname, c.relname, con.conname`

// indexesTemplate reads indexes with their columns in key order.
//
// pg_get_indexdef with a column number renders the key element, which is a
// column name for an ordinary index and an expression for a functional one. The
// expression is kept as written: an index on lower(email) is a fact about the
// table, and dropping it because it is not a bare column would hide it.
const indexesTemplate = `
SELECT n.nspname,
       c.relname,
       i.relname,
       ix.indisunique,
       ix.indisprimary,
       am.amname,
       ARRAY(SELECT pg_get_indexdef(ix.indexrelid, k, false)
             FROM generate_series(1, ix.indnkeyatts) AS k),
       ARRAY(SELECT (ix.indoption[k - 1] & 1) = 1
             FROM generate_series(1, ix.indnkeyatts) AS k)
FROM pg_index ix
JOIN pg_class c ON c.oid = ix.indrelid
JOIN pg_class i ON i.oid = ix.indexrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_am am ON am.oid = i.relam
WHERE ` + predicatePlaceholder + `
ORDER BY n.nspname, c.relname, i.relname`

// The remaining catalog queries have no per-table form: they are read once for
// the whole survey either way.
const (
	versionQuery = `SELECT version()`

	databaseInfoQuery = `
SELECT pg_encoding_to_char(d.encoding),
       d.datcollate,
       pg_get_userbyid(d.datdba),
       pg_database_size(d.datname),
       d.datistemplate
FROM pg_database AS d
WHERE d.datname = current_database()`

	// A survey covers every schema that is not one the engine provides. The
	// pg_ prefix covers pg_catalog, pg_toast, and the per-session pg_temp
	// namespaces in one condition.
	schemasQuery = `
SELECT n.nspname
FROM pg_namespace AS n
WHERE n.nspname NOT LIKE 'pg\_%' AND n.nspname <> 'information_schema'
ORDER BY n.nspname`

	tablesQuery = `
SELECT n.nspname, c.relname, c.reltuples, obj_description(c.oid, 'pg_class')
FROM pg_class AS c
JOIN pg_namespace AS n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'p') AND n.nspname = ANY($1)
ORDER BY n.nspname, c.relname`

	viewsQuery = `
SELECT n.nspname, c.relname, pg_get_viewdef(c.oid, true), obj_description(c.oid, 'pg_class')
FROM pg_class AS c
JOIN pg_namespace AS n ON n.oid = c.relnamespace
WHERE c.relkind IN ('v', 'm') AND n.nspname = ANY($1)
ORDER BY n.nspname, c.relname`

	// prokind 'f' is a plain function and 'p' a procedure. Aggregates and window
	// functions are excluded because pg_get_functiondef raises an error on them
	// rather than returning NULL.
	routinesQuery = `
SELECT n.nspname,
       p.proname,
       p.prokind::text,
       l.lanname,
       pg_get_function_result(p.oid),
       pg_get_function_identity_arguments(p.oid),
       obj_description(p.oid, 'pg_proc')
FROM pg_proc AS p
JOIN pg_namespace AS n ON n.oid = p.pronamespace
JOIN pg_language AS l ON l.oid = p.prolang
WHERE p.prokind IN ('f', 'p') AND n.nspname = ANY($1)
ORDER BY n.nspname, p.proname`

	// tgisinternal marks the triggers the engine creates to enforce foreign
	// keys. They are an implementation detail of a constraint already in the
	// document, not something an operator declared.
	triggersQuery = `
SELECT n.nspname, c.relname, t.tgname, t.tgtype, pg_get_triggerdef(t.oid, true)
FROM pg_trigger AS t
JOIN pg_class AS c ON c.oid = t.tgrelid
JOIN pg_namespace AS n ON n.oid = c.relnamespace
WHERE NOT t.tgisinternal AND n.nspname = ANY($1)
ORDER BY n.nspname, c.relname, t.tgname`

	// The typrelid condition excludes the composite type PostgreSQL creates for
	// every table's row shape, which is not a user-defined type in any sense an
	// operator means.
	userTypesQuery = `
SELECT n.nspname,
       t.typname,
       t.typtype::text,
       COALESCE(string_agg(e.enumlabel, ', ' ORDER BY e.enumsortorder), '')
FROM pg_type AS t
JOIN pg_namespace AS n ON n.oid = t.typnamespace
LEFT JOIN pg_enum AS e ON e.enumtypid = t.oid
WHERE t.typtype IN ('e', 'c', 'd', 'r')
  AND n.nspname = ANY($1)
  AND (t.typrelid = 0 OR EXISTS (
        SELECT 1 FROM pg_class AS c WHERE c.oid = t.typrelid AND c.relkind = 'c'))
GROUP BY n.nspname, t.typname, t.typtype
ORDER BY n.nspname, t.typname`

	databasesQuery = `
SELECT d.datname, d.datistemplate
FROM pg_database AS d
WHERE d.datallowconn
ORDER BY d.datname`

	samplingColumnsQuery = `
SELECT a.attname,
       -- A column with no default makes pg_get_expr NULL, and NULL LIKE '...'
       -- is NULL rather than false -- so the whole disjunction is NULL and the
       -- scan into a bool fails. Every column without a default hits this.
       COALESCE(a.attidentity <> '' OR pg_get_expr(d.adbin, d.adrelid) LIKE 'nextval(%', false),
       COALESCE(ix.indisprimary, false)
FROM pg_attribute AS a
JOIN pg_class AS c ON c.oid = a.attrelid
JOIN pg_namespace AS n ON n.oid = c.relnamespace
LEFT JOIN pg_attrdef AS d ON d.adrelid = c.oid AND d.adnum = a.attnum
LEFT JOIN pg_index AS ix ON ix.indrelid = c.oid AND ix.indisprimary AND a.attnum = ANY(ix.indkey)
WHERE a.attnum > 0 AND NOT a.attisdropped AND n.nspname = $1 AND c.relname = $2
ORDER BY a.attnum`
)

// withPredicate completes a query template with one of the two predicates.
//
// It is a substitution over package constants, never over a caller's value, so
// there is no injection surface here: the schema and table names themselves
// always travel as bound parameters.
func withPredicate(template, predicate string) string {
	return strings.Replace(template, predicatePlaceholder, predicate, 1)
}
