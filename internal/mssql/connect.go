// Package mssql surveys a Microsoft SQL Server database.
//
// The driver is microsoft/go-mssqldb, which is pure Go and therefore satisfies
// R13: no cgo anywhere in the graph, so one static binary reaches an airgapped
// host with no vendor client library present.
//
// The retired Rust implementation shipped a placeholder for this engine because
// no usable pure-Rust driver existed. There is no placeholder concept in this
// tree; an engine is either implemented or absent from the wiring in cmd/.
package mssql

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"time"

	_ "github.com/microsoft/go-mssqldb" // registers the "sqlserver" driver

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// driverName is what microsoft/go-mssqldb registers itself as.
const driverName = "sqlserver"

// defaultPort is the port SQL Server listens on when the configuration names
// none.
const defaultPort uint16 = 1433

// ErrClosed reports use of an adapter whose pool has been closed.
var ErrClosed = errors.New("mssql: adapter is closed")

// ErrNoDatabase reports a configuration that names no database to survey.
var ErrNoDatabase = errors.New("mssql: no database name was given")

// ErrUnknownTable reports a table the catalog does not have.
var ErrUnknownTable = errors.New("mssql: no such table or view")

// Adapter surveys one SQL Server database.
type Adapter struct {
	db           *sql.DB
	host         string
	port         *uint16
	database     string
	username     string
	queryTimeout time.Duration
}

// Adapter satisfies the adapter contract. The assertion is here rather than in a
// test so that a signature drift is a build failure in this package.
var _ dbadapter.Adapter = (*Adapter)(nil)

// Open connects to the configured database and verifies the connection is
// usable.
func Open(ctx context.Context, cfg dbadapter.ConnectionConfig) (*Adapter, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mssql: invalid connection configuration: %w", err)
	}

	if cfg.Database == "" {
		return nil, ErrNoDatabase
	}

	db, err := sql.Open(driverName, dataSourceName(cfg))
	if err != nil {
		return nil, fmt.Errorf("mssql: open %s: %w", cfg.Host, err)
	}

	db.SetMaxOpenConns(int(cfg.MaxConnections))
	db.SetMaxIdleConns(int(cfg.MinIdleConnections))
	db.SetConnMaxIdleTime(cfg.MaxIdleTime)

	adapter := &Adapter{
		db:           db,
		host:         cfg.Host,
		port:         cfg.Port,
		database:     cfg.Database,
		username:     cfg.Username,
		queryTimeout: cfg.QueryTimeout,
	}

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

// DatabaseType reports SQL Server.
func (a *Adapter) DatabaseType() dbschema.DatabaseType {
	return dbschema.SQLServer
}

// Supports reports whether SQL Server has a capability.
func (a *Adapter) Supports(feature dbadapter.Feature) bool {
	return mssqlFeatures[feature]
}

// Ping verifies the server is reachable and the credential is accepted.
func (a *Adapter) Ping(ctx context.Context) error {
	if a.db == nil {
		return ErrClosed
	}

	if err := a.db.PingContext(ctx); err != nil {
		return fmt.Errorf("mssql: ping %s: %w", a.host, err)
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
		return fmt.Errorf("mssql: close %s: %w", a.host, err)
	}

	return nil
}

// mssqlFeatures records what the engine has. Every Feature is listed, including
// the absent ones, so that adding a capability to the contract makes the
// exhaustiveness check fail here rather than defaulting this engine to "no".
var mssqlFeatures = map[dbadapter.Feature]bool{
	dbadapter.FeatureSchemas:     true,
	dbadapter.FeatureViews:       true,
	dbadapter.FeatureRoutines:    true,
	dbadapter.FeatureTriggers:    true,
	dbadapter.FeatureCustomTypes: true,
	// sys.partitions carries a maintained row count for the heap or clustered
	// index, which is an estimate rather than a count but needs no scan.
	dbadapter.FeatureRowCountEstimate: true,
	// A SQL Server connection is bound to one database. Surveying another means
	// a second connection, which is a multi-database mode this adapter does not
	// implement; the schemas within one database are covered instead.
	dbadapter.FeatureMultiDatabase:   false,
	dbadapter.FeatureSchemaInference: false,
}

// dataSourceName builds the driver's URL-form DSN.
//
// The password is revealed here and nowhere else, and it is placed through
// url.UserPassword so that a password containing a delimiter is escaped rather
// than truncating the URL. The assembled string is handed straight to the driver
// and never logged or returned.
func dataSourceName(cfg dbadapter.ConnectionConfig) string {
	port := defaultPort
	if cfg.Port != nil {
		port = *cfg.Port
	}

	query := url.Values{}
	query.Set("database", cfg.Database)
	query.Set("connection timeout", strconv.Itoa(int(cfg.ConnectTimeout.Seconds())))

	if cfg.ReadOnly {
		// ApplicationIntent is honored by an availability-group listener, which
		// routes a read-only intent to a secondary replica that physically
		// cannot be written to. On a standalone server it is accepted and has no
		// effect -- see GOTCHAS: unlike the other engines, SQL Server has no
		// session-level read-only switch, so the guarantee there rests on this
		// package issuing only reads.
		query.Set("ApplicationIntent", "ReadOnly")
	}

	dsn := url.URL{
		Scheme:   driverName,
		User:     url.UserPassword(cfg.Username, cfg.Password.RevealString()),
		Host:     cfg.Host + ":" + strconv.FormatUint(uint64(port), 10),
		RawQuery: query.Encode(),
	}

	return dsn.String()
}

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
