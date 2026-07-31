package dbschema

import (
	"time"
)

// Schema is a complete schema document for one database. It is the unit that
// internal/artifact writes to disk and reads back, and the input to every
// operation in this package.
//
// Format and FormatVersion are written on every document; New sets them.
type Schema struct {
	Format        string       `json:"format"`
	FormatVersion string       `json:"format_version"`
	DatabaseInfo  DatabaseInfo `json:"database_info"`
	Tables        []Table      `json:"tables"`
	Views         []View       `json:"views"`
	// Indexes and Constraints are the schema-wide aggregate of the per-table
	// lists. AggregateIndexesAndConstraints populates them; they are redundant
	// with the per-table data by design, so a report can iterate them once
	// instead of walking every table.
	Indexes     []Index      `json:"indexes"`
	Constraints []Constraint `json:"constraints"`
	Procedures  []Routine    `json:"procedures"`
	Functions   []Routine    `json:"functions"`
	Triggers    []Trigger    `json:"triggers"`
	UserTypes   []UserType   `json:"user_types"`
	// Samples is nil when sampling was not requested, distinct from an empty
	// slice, which means sampling ran and returned nothing.
	Samples []TableSample `json:"samples,omitempty"`
	// QualityMetrics is nil when quality analysis did not run, distinct from an
	// empty slice, which means it ran over no samples. Metrics carry counts,
	// ratios, and column names only, never a sampled value.
	QualityMetrics     []TableQualityMetrics `json:"quality_metrics,omitempty"`
	CollectionMetadata CollectionMetadata    `json:"collection_metadata"`
}

// New returns an empty schema for the given database, stamped with the current
// format identifiers and collection time.
//
// collectorVersion is the version string of the binary performing the
// collection; the package does not read it from build info so that callers with
// no build metadata (tests, embedded uses) can supply their own.
func New(info DatabaseInfo, collectorVersion string, collectedAt time.Time) *Schema {
	return &Schema{
		Format:        Format,
		FormatVersion: FormatVersion,
		DatabaseInfo:  info,
		Tables:        []Table{},
		Views:         []View{},
		Indexes:       []Index{},
		Constraints:   []Constraint{},
		Procedures:    []Routine{},
		Functions:     []Routine{},
		Triggers:      []Trigger{},
		UserTypes:     []UserType{},
		CollectionMetadata: CollectionMetadata{
			CollectedAt:      collectedAt,
			CollectorVersion: collectorVersion,
			Warnings:         []string{},
		},
	}
}

// AggregateIndexesAndConstraints rebuilds the schema-wide Indexes and
// Constraints lists from the per-table lists. Call it once after all tables have
// been added.
func (s *Schema) AggregateIndexesAndConstraints() {
	indexCount, constraintCount := 0, 0
	for i := range s.Tables {
		indexCount += len(s.Tables[i].Indexes)
		constraintCount += len(s.Tables[i].Constraints)
	}

	indexes := make([]Index, 0, indexCount)
	constraints := make([]Constraint, 0, constraintCount)

	for i := range s.Tables {
		indexes = append(indexes, s.Tables[i].Indexes...)
		constraints = append(constraints, s.Tables[i].Constraints...)
	}

	s.Indexes = indexes
	s.Constraints = constraints
}

// AddWarning records a non-fatal problem encountered during collection, such as
// a permission error on one table.
func (s *Schema) AddWarning(warning string) {
	s.CollectionMetadata.Warnings = append(s.CollectionMetadata.Warnings, warning)
}

// ObjectCount is the total number of catalog objects in the document.
func (s *Schema) ObjectCount() int {
	return len(s.Tables) +
		len(s.Views) +
		len(s.Indexes) +
		len(s.Constraints) +
		len(s.Procedures) +
		len(s.Functions) +
		len(s.Triggers) +
		len(s.UserTypes)
}

// DatabaseInfo identifies the database a schema was collected from and records
// how much of it the collecting credential could see.
type DatabaseInfo struct {
	Name string `json:"name"`
	// Type is the engine the database runs on.
	Type DatabaseType `json:"type"`
	// Version is the engine version string, for example "16.2".
	Version *string `json:"version,omitempty"`
	// SizeBytes is the on-disk estimate the engine reports, where it reports one.
	SizeBytes *uint64 `json:"size_bytes,omitempty"`
	Encoding  *string `json:"encoding,omitempty"`
	Collation *string `json:"collation,omitempty"`
	Owner     *string `json:"owner,omitempty"`
	// SystemDatabase marks engine-provided databases such as postgres,
	// template0, and information_schema.
	SystemDatabase   bool             `json:"system_database"`
	AccessLevel      AccessLevel      `json:"access_level"`
	CollectionStatus CollectionStatus `json:"collection_status"`
}

// NewDatabaseInfo returns database info for a fully readable database that
// collected successfully. Callers narrow the access level and status as they
// discover otherwise.
func NewDatabaseInfo(name string, engine DatabaseType) DatabaseInfo {
	return DatabaseInfo{
		Name:             name,
		Type:             engine,
		AccessLevel:      AccessFull,
		CollectionStatus: CollectionSucceeded(),
	}
}

// CollectionState is the discriminator of a CollectionStatus.
type CollectionState string

// The outcomes a per-database collection can report.
const (
	CollectionSuccess CollectionState = "success"
	CollectionFailed  CollectionState = "failed"
	CollectionSkipped CollectionState = "skipped"
)

var collectionStates = map[CollectionState]struct{}{
	CollectionSuccess: {},
	CollectionFailed:  {},
	CollectionSkipped: {},
}

// Valid reports whether c is a recognized collection state.
func (c CollectionState) Valid() bool {
	_, ok := collectionStates[c]

	return ok
}

// UnmarshalJSON rejects unrecognized collection states.
func (c *CollectionState) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "collection state", c)
}

// CollectionStatus is the outcome of collecting one database. Error belongs to
// CollectionFailed and Reason to CollectionSkipped. The payload fields are
// pointers so the field belonging to another state is absent from the
// marshalled document rather than present as an empty string.
//
// Error carries an operator-facing failure description. It must never carry a
// connection string or any other credential-bearing value: the credential scan
// in this package treats the document as untrusted and rejects a document whose
// error text embeds one.
//
// Build these with CollectionSucceeded, CollectionFailure, and CollectionSkip.
type CollectionStatus struct {
	State  CollectionState `json:"state"`
	Error  *string         `json:"error,omitempty"`
	Reason *string         `json:"reason,omitempty"`
}

// CollectionSucceeded builds a status reporting a complete collection.
func CollectionSucceeded() CollectionStatus {
	return CollectionStatus{State: CollectionSuccess}
}

// CollectionFailure builds a status reporting a failed collection.
func CollectionFailure(message string) CollectionStatus {
	return CollectionStatus{State: CollectionFailed, Error: &message}
}

// CollectionSkip builds a status reporting a database that was not collected.
func CollectionSkip(reason string) CollectionStatus {
	return CollectionStatus{State: CollectionSkipped, Reason: &reason}
}

// CollectionMetadata records when and by what a document was produced.
type CollectionMetadata struct {
	CollectedAt time.Time `json:"collected_at"`
	// DurationMS is the wall-clock duration of the collection in milliseconds.
	DurationMS uint64 `json:"duration_ms"`
	// CollectorVersion is the version of the binary that produced the document.
	CollectorVersion string `json:"collector_version"`
	// Warnings are non-fatal problems, such as tables the credential could not read.
	Warnings []string `json:"warnings"`
}

// SetDuration records how long a collection took.
//
// It takes a Duration rather than a count of milliseconds so the unit is stated
// by the type instead of being a convention every adapter has to remember, and
// it refuses a negative one: a clock that ran backwards is not a negative
// runtime.
func (m *CollectionMetadata) SetDuration(elapsed time.Duration) {
	if elapsed <= 0 {
		m.DurationMS = 0

		return
	}

	//nolint:gosec // G115: the guard above rules out the only negative case.
	m.DurationMS = uint64(elapsed.Milliseconds())
}

// ServerSchema is the document produced by a multi-database collection: server
// facts plus one Schema per database reached.
type ServerSchema struct {
	Format             string             `json:"format"`
	FormatVersion      string             `json:"format_version"`
	ServerInfo         ServerInfo         `json:"server_info"`
	Databases          []Schema           `json:"databases"`
	CollectionMetadata CollectionMetadata `json:"collection_metadata"`
}

// NewServerSchema returns an empty server document stamped with the current
// format identifiers.
func NewServerSchema(info ServerInfo, collectorVersion string, collectedAt time.Time) *ServerSchema {
	return &ServerSchema{
		Format:        ServerFormat,
		FormatVersion: FormatVersion,
		ServerInfo:    info,
		Databases:     []Schema{},
		CollectionMetadata: CollectionMetadata{
			CollectedAt:      collectedAt,
			CollectorVersion: collectorVersion,
			Warnings:         []string{},
		},
	}
}

// ServerFormat is the document discriminator for a multi-database document. It
// differs from Format so a reader can tell the two apart without inspecting the
// rest of the document.
const ServerFormat = "dbsurveyor/server-schema"

// ServerInfo describes the server a multi-database collection ran against.
//
// ConnectionUser is the account name only. The connection string it came from is
// never carried into a document.
type ServerInfo struct {
	ServerType DatabaseType `json:"server_type"`
	Version    string       `json:"version"`
	Host       string       `json:"host"`
	Port       *uint16      `json:"port,omitempty"`
	// TotalDatabases is every database discovered on the server, including any
	// excluded or skipped.
	TotalDatabases int `json:"total_databases"`
	// CollectedDatabases is the count actually collected.
	CollectedDatabases int `json:"collected_databases"`
	// SystemDatabasesExcluded is the count skipped for being engine-provided.
	SystemDatabasesExcluded int            `json:"system_databases_excluded"`
	ConnectionUser          string         `json:"connection_user"`
	Superuser               bool           `json:"superuser"`
	CollectionMode          CollectionMode `json:"collection_mode"`
}

// CollectionModeKind is the discriminator of a CollectionMode.
type CollectionModeKind string

// The modes a collection run can operate in.
const (
	ModeSingleDatabase CollectionModeKind = "single_database"
	ModeMultiDatabase  CollectionModeKind = "multi_database"
)

var collectionModeKinds = map[CollectionModeKind]struct{}{
	ModeSingleDatabase: {},
	ModeMultiDatabase:  {},
}

// Valid reports whether k is a recognized collection mode kind.
func (k CollectionModeKind) Valid() bool {
	_, ok := collectionModeKinds[k]

	return ok
}

// UnmarshalJSON rejects unrecognized collection mode kinds.
func (k *CollectionModeKind) UnmarshalJSON(data []byte) error {
	return unmarshalEnum(data, "collection mode kind", k)
}

// CollectionMode records whether one database or a whole server was collected,
// and for a server, how the discovered databases fared.
type CollectionMode struct {
	Kind       CollectionModeKind `json:"kind"`
	Discovered *int               `json:"discovered,omitempty"`
	Collected  *int               `json:"collected,omitempty"`
	Failed     *int               `json:"failed,omitempty"`
}

// SingleDatabase builds a single-database collection mode.
func SingleDatabase() CollectionMode {
	return CollectionMode{Kind: ModeSingleDatabase}
}

// MultiDatabase builds a multi-database collection mode with the run's tallies.
func MultiDatabase(discovered, collected, failed int) CollectionMode {
	return CollectionMode{
		Kind:       ModeMultiDatabase,
		Discovered: &discovered,
		Collected:  &collected,
		Failed:     &failed,
	}
}
