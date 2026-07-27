package dbschema

// Table is a single table as reported by the engine catalog.
type Table struct {
	Name string `json:"name"`
	// Schema is the namespace the table lives in: "public" on PostgreSQL, the
	// database name on MySQL, nil on SQLite, which has no namespace layer.
	Schema      *string      `json:"schema,omitempty"`
	Columns     []Column     `json:"columns"`
	PrimaryKey  *PrimaryKey  `json:"primary_key,omitempty"`
	ForeignKeys []ForeignKey `json:"foreign_keys"`
	Indexes     []Index      `json:"indexes"`
	Constraints []Constraint `json:"constraints"`
	Comment     *string      `json:"comment,omitempty"`
	// RowCount is an estimate drawn from engine statistics. It may be stale, and
	// engines that do not maintain such statistics report nil rather than zero,
	// so a nil count and an empty table are distinguishable.
	RowCount *uint64 `json:"row_count,omitempty"`
}

// Column is a single column of a table or view.
type Column struct {
	Name     string          `json:"name"`
	DataType UnifiedDataType `json:"data_type"`
	// Nullable is taken from the schema definition, not from sampled data.
	Nullable     bool `json:"nullable"`
	PrimaryKey   bool `json:"primary_key"`
	AutoGenerate bool `json:"auto_generate"`
	// Default is the SQL expression for the column default exactly as the
	// catalog reports it. It is not evaluated.
	Default *string `json:"default,omitempty"`
	Comment *string `json:"comment,omitempty"`
	// OrdinalPosition is the 1-based position of the column within its table.
	// Engines that report a 0-based index are normalized by their adapter.
	OrdinalPosition uint32 `json:"ordinal_position"`
}

// PrimaryKey is a table's primary key. Columns are in key order.
type PrimaryKey struct {
	Name    *string  `json:"name,omitempty"`
	Columns []string `json:"columns"`
}

// ForeignKey is a foreign key constraint. Columns and ReferencedColumns are
// positionally paired: the nth local column references the nth parent column.
type ForeignKey struct {
	Name    *string  `json:"name,omitempty"`
	Columns []string `json:"columns"`
	// ReferencedTable is the unqualified name of the parent table.
	ReferencedTable string `json:"referenced_table"`
	// ReferencedSchema is set when the parent table lives in a different
	// namespace than the child.
	ReferencedSchema  *string  `json:"referenced_schema,omitempty"`
	ReferencedColumns []string `json:"referenced_columns"`
	// OnDelete and OnUpdate are nil when the engine reports no explicit action,
	// which means the engine default applies. Nil is not equivalent to NoAction.
	OnDelete *ReferentialAction `json:"on_delete,omitempty"`
	OnUpdate *ReferentialAction `json:"on_update,omitempty"`
}

// Index is a single index. Columns are in key order, which defines the usable
// key prefixes.
type Index struct {
	Name      string        `json:"name"`
	TableName string        `json:"table_name"`
	Schema    *string       `json:"schema,omitempty"`
	Columns   []IndexColumn `json:"columns"`
	Unique    bool          `json:"unique"`
	// Primary marks the index that backs the table's primary key.
	Primary bool `json:"primary"`
	// IndexType is the engine-specific access method, for example "btree",
	// "hash", or "gin".
	IndexType *string `json:"index_type,omitempty"`
}

// IndexColumn is one column of an index, with its sort direction when the
// engine reports one.
type IndexColumn struct {
	Name      string         `json:"name"`
	SortOrder *SortDirection `json:"sort_order,omitempty"`
}

// Constraint is a named table constraint. CheckClause is populated only for
// ConstraintCheck.
type Constraint struct {
	Name           string         `json:"name"`
	TableName      string         `json:"table_name"`
	Schema         *string        `json:"schema,omitempty"`
	ConstraintType ConstraintType `json:"constraint_type"`
	Columns        []string       `json:"columns"`
	CheckClause    *string        `json:"check_clause,omitempty"`
}
