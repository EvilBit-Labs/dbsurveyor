// Package sqlite surveys a SQLite database file.
//
// The driver is modernc.org/sqlite, a pure-Go translation of the SQLite
// amalgamation. mattn/go-sqlite3 wraps the C library and is faster, and it is
// rejected outright: it needs cgo, and R13 forbids cgo anywhere in the graph so
// that an operator can drop one static binary onto an airgapped host with no
// vendor client libraries present.
package sqlite

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"net/url"
	"os"
	"strings"
	"time"

	_ "modernc.org/sqlite" // registers the "sqlite" driver with database/sql

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// driverName is what modernc.org/sqlite registers itself as.
const driverName = "sqlite"

// busyTimeout bounds how long a statement waits for a lock another connection
// holds. A survey is read-only, so it only ever contends with somebody else's
// writer; waiting a few seconds beats failing the run.
const busyTimeout = 5 * time.Second

// ErrClosed reports use of an adapter whose pool has been closed.
var ErrClosed = errors.New("sqlite: adapter is closed")

// ErrUnknownTable reports a table or view the catalog does not have. PRAGMA
// table_info reports an unknown table as an empty result rather than as an
// error, so the empty result is turned back into one here: a caller sampling a
// misspelled table needs to be told, not handed zero columns.
var ErrUnknownTable = errors.New("sqlite: no such table or view")

// Adapter surveys one SQLite database file.
//
// SQLite has no server, so ConnectionConfig.Host carries the path to the
// database file rather than a hostname. Nothing else in the connection
// configuration applies: there is no port, no username, and no password.
type Adapter struct {
	db           *sql.DB
	path         string
	queryTimeout time.Duration
}

// Adapter satisfies the adapter contract. The assertion is here rather than in a
// test so that a signature drift is a build failure in this package.
var _ dbadapter.Adapter = (*Adapter)(nil)

// Open connects to the database file named by cfg.Host and verifies it is
// readable.
//
// The connection is opened with PRAGMA query_only when cfg.ReadOnly is set,
// which makes the engine itself refuse a write on this connection. That is a
// second line of defense: every statement this package issues is a read
// regardless, and the pragma is what catches the case where one is not.
func Open(ctx context.Context, cfg dbadapter.ConnectionConfig) (*Adapter, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("sqlite: invalid connection configuration: %w", err)
	}

	// The driver creates a database file that is not there, which for a survey
	// is never what was wanted: a mistyped path would produce a valid, empty,
	// entirely fictional schema document. query_only stops writes to the
	// contents but not the creation of the file itself, so existence is checked
	// before the file is opened.
	if _, err := os.Stat(cfg.Host); err != nil {
		return nil, fmt.Errorf("sqlite: %w", err)
	}

	db, err := sql.Open(driverName, dataSourceName(cfg))
	if err != nil {
		return nil, fmt.Errorf("sqlite: open %s: %w", cfg.Host, err)
	}

	// A SQLite database is a file, so a large pool buys contention rather than
	// throughput.
	db.SetMaxOpenConns(int(cfg.MaxConnections))
	db.SetMaxIdleConns(int(cfg.MinIdleConnections))
	db.SetConnMaxIdleTime(cfg.MaxIdleTime)

	adapter := &Adapter{db: db, path: cfg.Host, queryTimeout: cfg.QueryTimeout}

	connect, cancel := context.WithTimeout(ctx, cfg.ConnectTimeout)
	defer cancel()

	if err := adapter.Ping(connect); err != nil {
		// The pool never became usable, so the close error says nothing an
		// operator can act on and the open error is the one they need.
		discardError(db.Close())

		return nil, err
	}

	return adapter, nil
}

// DatabaseType reports SQLite.
func (a *Adapter) DatabaseType() dbschema.DatabaseType {
	return dbschema.SQLite
}

// Supports reports whether SQLite has a capability.
func (a *Adapter) Supports(feature dbadapter.Feature) bool {
	return sqliteFeatures[feature]
}

// Ping verifies the database file is open and readable.
func (a *Adapter) Ping(ctx context.Context) error {
	if a.db == nil {
		return ErrClosed
	}

	if err := a.db.PingContext(ctx); err != nil {
		return fmt.Errorf("sqlite: ping %s: %w", a.path, err)
	}

	return nil
}

// Close releases the pool. It is safe to call more than once.
func (a *Adapter) Close() error {
	if a.db == nil {
		return nil
	}

	db := a.db
	a.db = nil

	if err := db.Close(); err != nil {
		return fmt.Errorf("sqlite: close %s: %w", a.path, err)
	}

	return nil
}

// sqliteFeatures records what the engine has. Every Feature is listed, including
// the absent ones, so that adding a capability to the contract makes the
// exhaustiveness check fail here rather than defaulting this engine to "no".
var sqliteFeatures = map[dbadapter.Feature]bool{
	dbadapter.FeatureSchemas: false,
	dbadapter.FeatureViews:   true,
	// SQLite has no stored procedures or functions in the catalog. Application
	// -defined functions are registered by the connecting process and are not
	// part of the database file.
	dbadapter.FeatureRoutines:    false,
	dbadapter.FeatureTriggers:    true,
	dbadapter.FeatureCustomTypes: false,
	// MAX(rowid) is an index seek, not a scan, so an estimate is cheap.
	dbadapter.FeatureRowCountEstimate: true,
	dbadapter.FeatureMultiDatabase:    false,
	dbadapter.FeatureSchemaInference:  false,
}

// dataSourceName builds the DSN for the configured file.
//
// The query parameters are pragmas the driver applies on every new connection.
// They are pragmas rather than open flags because the driver applies them
// through the same statement path a query takes, so a rejected pragma surfaces
// as an error on connect rather than as a silently ignored option.
func dataSourceName(cfg dbadapter.ConnectionConfig) string {
	pragmas := []string{fmt.Sprintf("busy_timeout(%d)", busyTimeout.Milliseconds())}

	if cfg.ReadOnly {
		pragmas = append(pragmas, "query_only(1)")
	}

	values := url.Values{"_pragma": pragmas}

	return cfg.Host + "?" + values.Encode()
}

// databaseName is the name a document records for this file: the file's base
// name without its extension, since SQLite has no name of its own for it.
func databaseName(path string) string {
	name := path
	if cut := strings.LastIndexAny(name, `/\`); cut >= 0 {
		name = name[cut+1:]
	}

	if dot := strings.LastIndexByte(name, '.'); dot > 0 {
		name = name[:dot]
	}

	if name == "" {
		return path
	}

	return name
}

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
