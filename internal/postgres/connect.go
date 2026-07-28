// Package postgres surveys a PostgreSQL database.
//
// The driver is jackc/pgx/v5 used through its native pool rather than through
// database/sql. The native API is what makes the batch collection in batch.go
// worth writing: it exposes PostgreSQL array parameters directly, so five
// queries can each cover every table in the survey instead of being issued once
// per table.
//
// pgx is pure Go, which is what R13 requires: no cgo anywhere in the graph, so
// one static binary reaches an airgapped host with no vendor client library
// present.
package postgres

import (
	"context"
	"errors"
	"fmt"
	"math"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// defaultPort is the port PostgreSQL listens on when the configuration names
// none.
const defaultPort uint16 = 5432

// ErrClosed reports use of an adapter whose pool has been closed.
var ErrClosed = errors.New("postgres: adapter is closed")

// ErrNoDatabase reports a configuration that names no database to survey.
var ErrNoDatabase = errors.New("postgres: no database name was given")

// ErrUnknownTable reports a table the catalog does not have.
var ErrUnknownTable = errors.New("postgres: no such table or view")

// Adapter surveys one PostgreSQL database.
type Adapter struct {
	pool         *pgxpool.Pool
	host         string
	port         *uint16
	database     string
	username     string
	queryTimeout time.Duration
	// searchSchemas are the namespaces a survey covers. It is empty by default,
	// which means every schema that is not one of the engine's own.
	searchSchemas []string
}

// Adapter satisfies both contracts. The assertions are here rather than in a
// test so that a signature drift is a build failure in this package.
var (
	_ dbadapter.Adapter              = (*Adapter)(nil)
	_ dbadapter.MultiDatabaseAdapter = (*Adapter)(nil)
)

// Open connects to the configured database and verifies the connection is
// usable.
func Open(ctx context.Context, cfg dbadapter.ConnectionConfig) (*Adapter, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("postgres: invalid connection configuration: %w", err)
	}

	if cfg.Database == "" {
		return nil, ErrNoDatabase
	}

	poolConfig, err := poolConfiguration(cfg)
	if err != nil {
		return nil, err
	}

	connect, cancel := context.WithTimeout(ctx, cfg.ConnectTimeout)
	defer cancel()

	pool, err := pgxpool.NewWithConfig(connect, poolConfig)
	if err != nil {
		return nil, fmt.Errorf("postgres: connect to %s: %w", cfg.Host, err)
	}

	adapter := &Adapter{
		pool:         pool,
		host:         cfg.Host,
		port:         cfg.Port,
		database:     cfg.Database,
		username:     cfg.Username,
		queryTimeout: cfg.QueryTimeout,
	}

	if err := adapter.Ping(connect); err != nil {
		// The pool never became usable, so the open error is the one an operator
		// needs; Close reports nothing they can act on.
		pool.Close()

		return nil, err
	}

	return adapter, nil
}

// DatabaseType reports PostgreSQL.
func (a *Adapter) DatabaseType() dbschema.DatabaseType {
	return dbschema.PostgreSQL
}

// Supports reports whether PostgreSQL has a capability.
func (a *Adapter) Supports(feature dbadapter.Feature) bool {
	return postgresFeatures[feature]
}

// Ping verifies the server is reachable and the credential is accepted.
func (a *Adapter) Ping(ctx context.Context) error {
	if a.pool == nil {
		return ErrClosed
	}

	if err := a.pool.Ping(ctx); err != nil {
		return fmt.Errorf("postgres: ping %s: %w", a.host, err)
	}

	return nil
}

// Close releases the pool. It is safe to call more than once.
func (a *Adapter) Close() error {
	if a.pool == nil {
		return nil
	}

	pool := a.pool
	a.pool = nil

	pool.Close()

	return nil
}

// postgresFeatures records what the engine has. Every Feature is listed,
// including the absent ones, so that adding a capability to the contract makes
// the exhaustiveness check fail here rather than defaulting this engine to "no".
var postgresFeatures = map[dbadapter.Feature]bool{
	dbadapter.FeatureSchemas:     true,
	dbadapter.FeatureViews:       true,
	dbadapter.FeatureRoutines:    true,
	dbadapter.FeatureTriggers:    true,
	dbadapter.FeatureCustomTypes: true,
	// reltuples is maintained by ANALYZE and by autovacuum. It is an estimate,
	// and on a table that has never been analyzed it is -1 rather than a count.
	dbadapter.FeatureRowCountEstimate: true,
	dbadapter.FeatureMultiDatabase:    true,
	dbadapter.FeatureSchemaInference:  false,
}

// poolConfiguration builds the pgx pool configuration for a connection.
//
// The password is revealed here and nowhere else. It goes straight into the
// driver's configuration and is never formatted, logged, or joined into a
// connection string this code holds: the whole point of the Secret type is that
// the value has exactly one exit, and this is it.
func poolConfiguration(cfg dbadapter.ConnectionConfig) (*pgxpool.Config, error) {
	// The configuration is built from an empty connection string rather than
	// from a composed DSN, so no string that ever holds the password exists.
	poolConfig, err := pgxpool.ParseConfig("")
	if err != nil {
		return nil, fmt.Errorf("postgres: build pool configuration: %w", err)
	}

	port := defaultPort
	if cfg.Port != nil {
		port = *cfg.Port
	}

	poolConfig.ConnConfig.Host = cfg.Host
	poolConfig.ConnConfig.Port = port
	poolConfig.ConnConfig.Database = cfg.Database
	poolConfig.ConnConfig.User = cfg.Username
	poolConfig.ConnConfig.Password = cfg.Password.RevealString()
	poolConfig.ConnConfig.ConnectTimeout = cfg.ConnectTimeout
	poolConfig.MaxConns = poolSize(cfg.MaxConnections)
	poolConfig.MinConns = poolSize(cfg.MinIdleConnections)
	poolConfig.MaxConnIdleTime = cfg.MaxIdleTime

	if cfg.ReadOnly {
		// default_transaction_read_only makes the server refuse a write on this
		// session, which is the second line of defense behind every statement in
		// this package already being a read. It is a startup parameter rather
		// than a SET, so it applies from the first statement on every connection
		// the pool opens, including ones opened later to grow the pool.
		poolConfig.ConnConfig.RuntimeParams["default_transaction_read_only"] = "on"
	}

	return poolConfig, nil
}

// poolSize converts a configured connection count to the width pgx wants.
//
// The configuration carries an unsigned count and pgx wants a signed one, so a
// value past the signed maximum has no representation. Clamping is right rather
// than wrapping: a caller asking for more connections than an int32 can hold
// wants as many as possible, and wrapping would hand pgx a negative pool size.
func poolSize(connections uint32) int32 {
	if connections > math.MaxInt32 {
		return math.MaxInt32
	}

	return int32(connections)
}
