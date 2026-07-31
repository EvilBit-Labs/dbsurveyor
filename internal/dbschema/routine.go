package dbschema

// View is a view definition. Definition is nil when the collecting credential
// lacks permission to read the view body, which is a common outcome on managed
// engines and is not an error.
type View struct {
	Name       string   `json:"name"`
	Schema     *string  `json:"schema,omitempty"`
	Definition *string  `json:"definition,omitempty"`
	Columns    []Column `json:"columns"`
	Comment    *string  `json:"comment,omitempty"`
}

// Routine is a stored procedure or function. Both are carried by the same type
// because the engines differ on whether a routine returning a value is called a
// function, a procedure, or both; the distinction is preserved by which list on
// Schema the routine appears in.
type Routine struct {
	Name       string      `json:"name"`
	Schema     *string     `json:"schema,omitempty"`
	Definition *string     `json:"definition,omitempty"`
	Parameters []Parameter `json:"parameters"`
	// ReturnType is nil for routines that return nothing.
	ReturnType *UnifiedDataType `json:"return_type,omitempty"`
	// Language is the routine body's language, for example "plpgsql" or "sql".
	Language *string `json:"language,omitempty"`
	Comment  *string `json:"comment,omitempty"`
}

// Parameter is one parameter of a routine.
type Parameter struct {
	Name      string             `json:"name"`
	DataType  UnifiedDataType    `json:"data_type"`
	Direction ParameterDirection `json:"direction"`
	Default   *string            `json:"default,omitempty"`
}

// Trigger is a trigger attached to a table.
type Trigger struct {
	Name       string        `json:"name"`
	TableName  string        `json:"table_name"`
	Schema     *string       `json:"schema,omitempty"`
	Event      TriggerEvent  `json:"event"`
	Timing     TriggerTiming `json:"timing"`
	Definition *string       `json:"definition,omitempty"`
}

// UserType is a user-defined type: an enum, composite, domain, or range.
//
// The name deliberately avoids "CustomType", which is the constructor for a
// UnifiedDataType of kind TypeCustom. The two are different things: this is a
// catalog object, that is a column's declared type.
type UserType struct {
	Name       string       `json:"name"`
	Schema     *string      `json:"schema,omitempty"`
	Definition string       `json:"definition"`
	Category   TypeCategory `json:"category"`
}
