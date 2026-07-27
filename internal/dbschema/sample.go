package dbschema

import (
	"time"
)

// SampleState is the discriminator of a SampleStatus.
type SampleState string

// The outcomes a sampling operation can report.
const (
	// SampleComplete means the requested rows were returned.
	SampleComplete SampleState = "complete"
	// SamplePartialRetry means the adapter retried with a reduced limit after
	// the original request failed or timed out.
	SamplePartialRetry SampleState = "partial_retry"
	// SampleSkipped means no rows were requested from the engine at all.
	SampleSkipped SampleState = "skipped"
)

var sampleStates = map[SampleState]struct{}{
	SampleComplete:     {},
	SamplePartialRetry: {},
	SampleSkipped:      {},
}

// Valid reports whether s is a recognized sample state.
func (s SampleState) Valid() bool {
	_, ok := sampleStates[s]

	return ok
}

// UnmarshalJSON rejects unrecognized sample states.
func (s *SampleState) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "sample state", s)
}

// SampleStatus is the outcome of a sampling operation. State is the
// discriminator; OriginalLimit belongs to SamplePartialRetry and Reason to
// SampleSkipped. The payload fields are pointers so that the field belonging to
// another state is absent from the marshalled document rather than present as a
// zero value.
//
// A nil *SampleStatus on a TableSample means the adapter did not report an
// outcome, which is distinct from reporting SampleComplete.
//
// Go has no sum type, so "only the payload belonging to State is set" is an
// invariant the constructors establish and Validate checks, not one the type
// system enforces. Build these with Complete, PartialRetry, and Skipped.
type SampleStatus struct {
	State SampleState `json:"state"`
	// OriginalLimit is the row limit originally requested, set only for
	// SamplePartialRetry.
	OriginalLimit *uint32 `json:"original_limit,omitempty"`
	// Reason explains why sampling was skipped, set only for SampleSkipped.
	Reason *string `json:"reason,omitempty"`
}

// Complete builds a SampleStatus reporting a successful sample.
func Complete() SampleStatus {
	return SampleStatus{State: SampleComplete}
}

// PartialRetry builds a SampleStatus reporting a retry at a reduced limit,
// recording the limit originally requested.
func PartialRetry(originalLimit uint32) SampleStatus {
	return SampleStatus{State: SamplePartialRetry, OriginalLimit: &originalLimit}
}

// Skipped builds a SampleStatus reporting that sampling was not attempted.
func Skipped(reason string) SampleStatus {
	return SampleStatus{State: SampleSkipped, Reason: &reason}
}

// SamplingKind is the discriminator of a SamplingStrategy.
type SamplingKind string

// The sampling strategies an adapter can apply.
const (
	// SampleMostRecent takes the newest rows under the table's ordering strategy.
	SampleMostRecent SamplingKind = "most_recent"
	// SampleRandom takes an engine-provided random subset.
	SampleRandom SamplingKind = "random"
	// SampleNone means no rows were requested.
	SampleNone SamplingKind = "none"
)

var samplingKinds = map[SamplingKind]struct{}{
	SampleMostRecent: {},
	SampleRandom:     {},
	SampleNone:       {},
}

// Valid reports whether k is a recognized sampling kind.
func (k SamplingKind) Valid() bool {
	_, ok := samplingKinds[k]

	return ok
}

// UnmarshalJSON rejects unrecognized sampling kinds.
func (k *SamplingKind) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "sampling kind", k)
}

// SamplingStrategy records how rows were selected. Limit is the requested row
// count and is absent for SampleNone.
type SamplingStrategy struct {
	Kind  SamplingKind `json:"kind"`
	Limit *uint32      `json:"limit,omitempty"`
}

// MostRecent builds a most-recent sampling strategy at the given row limit.
func MostRecent(limit uint32) SamplingStrategy {
	return SamplingStrategy{Kind: SampleMostRecent, Limit: &limit}
}

// Random builds a random sampling strategy at the given row limit.
func Random(limit uint32) SamplingStrategy {
	return SamplingStrategy{Kind: SampleRandom, Limit: &limit}
}

// NoSampling builds a strategy recording that no rows were requested.
func NoSampling() SamplingStrategy {
	return SamplingStrategy{Kind: SampleNone}
}

// OrderingKind is the discriminator of an OrderingStrategy.
type OrderingKind string

// The orderings an adapter can use to define "most recent" for a table.
const (
	// OrderPrimaryKey orders by the table's primary key columns.
	OrderPrimaryKey OrderingKind = "primary_key"
	// OrderTimestamp orders by a detected timestamp column.
	OrderTimestamp OrderingKind = "timestamp"
	// OrderAutoIncrement orders by a detected auto-generated numeric column.
	OrderAutoIncrement OrderingKind = "auto_increment"
	// OrderSystemRowID orders by an engine-internal row identifier, such as
	// SQLite's rowid.
	OrderSystemRowID OrderingKind = "system_row_id"
	// OrderUnordered means no reliable ordering was found, so "most recent" is
	// not meaningful for this table.
	OrderUnordered OrderingKind = "unordered"
)

var orderingKinds = map[OrderingKind]struct{}{
	OrderPrimaryKey:    {},
	OrderTimestamp:     {},
	OrderAutoIncrement: {},
	OrderSystemRowID:   {},
	OrderUnordered:     {},
}

// Valid reports whether k is a recognized ordering kind.
func (k OrderingKind) Valid() bool {
	_, ok := orderingKinds[k]

	return ok
}

// UnmarshalJSON rejects unrecognized ordering kinds.
func (k *OrderingKind) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "ordering kind", k)
}

// OrderingStrategy records how a sampled table was ordered. Columns belongs to
// OrderPrimaryKey; Column belongs to the single-column orderings; Direction is
// set only for OrderTimestamp.
type OrderingStrategy struct {
	Kind      OrderingKind   `json:"kind"`
	Columns   []string       `json:"columns,omitempty"`
	Column    *string        `json:"column,omitempty"`
	Direction *SortDirection `json:"direction,omitempty"`
}

// TableSample is the rows sampled from one table, with the metadata needed to
// judge how representative they are.
type TableSample struct {
	TableName  string  `json:"table_name"`
	SchemaName *string `json:"schema_name,omitempty"`
	// Rows are JSON objects mapping column name to sampled value. They are the
	// only part of a schema document that carries user data, and so are the
	// primary target of redaction and of the credential scan.
	Rows []map[string]any `json:"rows"`
	// SampleSize is the number of rows actually returned, which may be below
	// the requested limit.
	SampleSize uint32 `json:"sample_size"`
	// TotalRows is an estimate from engine statistics; nil when unavailable.
	TotalRows        *uint64           `json:"total_rows,omitempty"`
	SamplingStrategy SamplingStrategy  `json:"sampling_strategy"`
	Ordering         *OrderingStrategy `json:"ordering,omitempty"`
	CollectedAt      time.Time         `json:"collected_at"`
	Warnings         []string          `json:"warnings"`
	// Status is nil when the adapter reported no outcome.
	Status *SampleStatus `json:"status,omitempty"`
}

// ColumnNames returns the column names of the first sampled row, or nil when
// the sample is empty. Names come from the first row only: a sample whose rows
// have differing key sets is malformed, and Validate reports it.
func (t *TableSample) ColumnNames() []string {
	if len(t.Rows) == 0 {
		return nil
	}

	names := make([]string, 0, len(t.Rows[0]))
	for name := range t.Rows[0] {
		names = append(names, name)
	}

	return names
}
