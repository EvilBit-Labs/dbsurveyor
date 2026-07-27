// Package dbadapter defines the contract every database adapter implements,
// and the parameter types that contract is expressed in.
//
// It is a leaf: it imports internal/dbschema and nothing else from this tree.
// That is what breaks the cycle the previous implementation hit -- an adapter
// package must import the interface for its types, so if the interface package
// also held the factory that constructs adapters, it would have to import every
// adapter back. Construction lives in cmd/ instead, wired explicitly (R11), so
// nothing here knows which adapters exist.
package dbadapter

import (
	"context"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// Feature names a capability an engine may or may not have.
//
// Adapters report features rather than callers switching on the database type,
// so that "does this engine have stored procedures" is answered by the adapter
// that knows, in one place, instead of by a type switch that has to be updated
// every time an engine is added.
type Feature string

const (
	// FeatureSchemas means the engine groups tables into named schemas.
	// SQLite and MongoDB do not.
	FeatureSchemas Feature = "schemas"
	// FeatureViews means the engine has views.
	FeatureViews Feature = "views"
	// FeatureRoutines means the engine has stored procedures or functions.
	FeatureRoutines Feature = "routines"
	// FeatureTriggers means the engine has triggers.
	FeatureTriggers Feature = "triggers"
	// FeatureCustomTypes means the engine has user-defined types.
	FeatureCustomTypes Feature = "custom_types"
	// FeatureRowCountEstimate means the engine exposes a row-count estimate
	// from statistics, without a full scan.
	FeatureRowCountEstimate Feature = "row_count_estimate"
	// FeatureMultiDatabase means one connection can enumerate and survey the
	// server's other databases.
	FeatureMultiDatabase Feature = "multi_database"
	// FeatureSchemaInference means the engine stores no schema and one must be
	// inferred from documents. MongoDB is the case this exists for.
	FeatureSchemaInference Feature = "schema_inference"
)

// Adapter surveys one database.
//
// Every method takes a context, so a survey against an unresponsive server can
// be cancelled rather than waited out. Every method is read-only: an
// implementation that issues DML, creates a temporary object, or runs ANALYZE
// is a bug, not a trade-off. An operator may be pointed at production with
// credentials they are not supposed to write with.
//
// Implementations are constructed by explicit wiring in cmd/, never registered
// through init (R11). A registry would mean importing an adapter for its side
// effect, which makes the set of enabled engines invisible at the call site and
// links every driver into every binary whether or not it is reachable.
type Adapter interface {
	// DatabaseType reports the engine this adapter speaks to.
	DatabaseType() dbschema.DatabaseType

	// Supports reports whether the engine has a capability.
	Supports(feature Feature) bool

	// Ping verifies the connection is usable. It is separated from collection
	// so a run can fail on an unreachable server in a second rather than
	// part-way through a survey.
	Ping(ctx context.Context) error

	// CollectSchema reads the database's structure.
	//
	// The returned schema is complete but unsampled: rows are read by
	// SampleTable, if at all. A partial failure is reported as a warning on the
	// schema rather than as an error, so a survey of a database where some
	// objects are unreadable still produces the objects that were.
	CollectSchema(ctx context.Context, cfg CollectionConfig) (*dbschema.Schema, error)

	// SampleTable reads rows from one table.
	//
	// The returned sample carries a status saying whether it is complete, was
	// retried at a lower limit, or was skipped, so a caller can tell an empty
	// table from one that could not be read.
	SampleTable(ctx context.Context, table TableRef, cfg SamplingConfig) (dbschema.TableSample, error)

	// Close releases the connection pool. It is safe to call more than once.
	Close() error
}

// MultiDatabaseAdapter is implemented by adapters that can survey a whole
// server.
//
// It is a separate interface rather than two more methods on Adapter because
// SQLite has no second database to find and MongoDB's notion of one differs
// enough to be worth stating separately. A caller asks with a type assertion,
// having checked FeatureMultiDatabase.
type MultiDatabaseAdapter interface {
	Adapter

	// ListDatabases enumerates the databases on the server, excluding the
	// engine's own catalogs unless the configuration asks for them.
	ListDatabases(ctx context.Context, cfg CollectionConfig) ([]string, error)

	// CollectServerSchema surveys every database the configuration selects.
	//
	// Each database gets its own pool, closed when that database is done. A
	// database that cannot be read is recorded with a failure status rather
	// than aborting the server survey.
	CollectServerSchema(ctx context.Context, cfg CollectionConfig) (*dbschema.ServerSchema, error)
}
