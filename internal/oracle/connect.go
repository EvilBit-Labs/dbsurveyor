// Package oracle surveys an Oracle database.
//
// The driver is sijms/go-ora, which speaks the wire protocol directly. That is
// the airgap-relevant property and the reason this engine is reachable at all:
// every other Go Oracle driver binds to the Instant Client, which is a
// separately licensed native library an operator would have to install on the
// host first. go-ora needs nothing but the binary, which is what R13 is for.
//
// Supported servers are 12.1 and later. The floor is the row-limiting clause --
// FETCH FIRST n ROWS ONLY is 12.1 syntax, and the ROWNUM rewrite it replaces
// cannot be combined with ORDER BY without a subquery that changes which rows
// come back. go-ora's protocol coverage for servers older than that is also
// uneven enough that claiming them would be claiming something untested.
package oracle

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"time"

	goora "github.com/sijms/go-ora/v2"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// driverName is what go-ora registers itself as.
const driverName = "oracle"

// defaultPort is the port an Oracle listener uses when the configuration names
// none.
const defaultPort uint16 = 1521

// MinimumServerVersion is the oldest server this adapter claims to support. See
// the package comment for why.
const MinimumServerVersion = "12.1"

// ErrClosed reports use of an adapter whose pool has been closed.
var ErrClosed = errors.New("oracle: adapter is closed")

// ErrNoService reports a configuration that names no service to connect to.
var ErrNoService = errors.New("oracle: no service name was given")

// ErrUnknownTable reports a table the catalog does not have.
var ErrUnknownTable = errors.New("oracle: no such table or view")

// Adapter surveys one Oracle schema.
//
// ConnectionConfig.Database carries the service name, which is what an Oracle
// listener routes on. The schema surveyed is the connecting user's own unless
// SetOwner names another, because "the database" in Oracle's vocabulary is the
// whole instance and a survey of one is a survey of every application on it.
type Adapter struct {
	db           *sql.DB
	host         string
	port         *uint16
	service      string
	username     string
	owner        string
	queryTimeout time.Duration
}

// Adapter satisfies the adapter contract. The assertion is here rather than in a
// test so that a signature drift is a build failure in this package.
var _ dbadapter.Adapter = (*Adapter)(nil)

// Open connects to the configured service and verifies the connection is usable.
func Open(ctx context.Context, cfg dbadapter.ConnectionConfig) (*Adapter, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("oracle: invalid connection configuration: %w", err)
	}

	if cfg.Database == "" {
		return nil, ErrNoService
	}

	db, err := sql.Open(driverName, dataSourceName(cfg))
	if err != nil {
		return nil, fmt.Errorf("oracle: open %s: %w", cfg.Host, err)
	}

	db.SetMaxOpenConns(int(cfg.MaxConnections))
	db.SetMaxIdleConns(int(cfg.MinIdleConnections))
	db.SetConnMaxIdleTime(cfg.MaxIdleTime)

	adapter := &Adapter{
		db:           db,
		host:         cfg.Host,
		port:         cfg.Port,
		service:      cfg.Database,
		username:     cfg.Username,
		owner:        normalize(cfg.Username),
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

// SetOwner selects the schema to survey, defaulting to the connecting user's.
//
// The name is normalized the way the engine would have folded it, so an operator
// who writes `hr` on a command line reaches the HR schema rather than nothing.
func (a *Adapter) SetOwner(owner string) {
	if owner != "" {
		a.owner = normalize(owner)
	}
}

// DatabaseType reports Oracle.
func (a *Adapter) DatabaseType() dbschema.DatabaseType {
	return dbschema.Oracle
}

// Supports reports whether Oracle has a capability.
func (a *Adapter) Supports(feature dbadapter.Feature) bool {
	return oracleFeatures[feature]
}

// Ping verifies the server is reachable and the credential is accepted.
func (a *Adapter) Ping(ctx context.Context) error {
	if a.db == nil {
		return ErrClosed
	}

	if err := a.db.PingContext(ctx); err != nil {
		return fmt.Errorf("oracle: ping %s: %w", a.host, err)
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
		return fmt.Errorf("oracle: close %s: %w", a.host, err)
	}

	return nil
}

// oracleFeatures records what the engine has. Every Feature is listed, including
// the absent ones, so that adding a capability to the contract makes the
// exhaustiveness check fail here rather than defaulting this engine to "no".
var oracleFeatures = map[dbadapter.Feature]bool{
	dbadapter.FeatureSchemas:     true,
	dbadapter.FeatureViews:       true,
	dbadapter.FeatureRoutines:    true,
	dbadapter.FeatureTriggers:    true,
	dbadapter.FeatureCustomTypes: true,
	// ALL_TABLES.NUM_ROWS is maintained by the optimizer statistics job, so it
	// is an estimate and is NULL on a table nothing has gathered stats for.
	dbadapter.FeatureRowCountEstimate: true,
	// An Oracle instance holds schemas, not databases, and one connection
	// already reaches all of them. Enumeration is by owner rather than by
	// database, which the multi-database contract does not describe.
	dbadapter.FeatureMultiDatabase:   false,
	dbadapter.FeatureSchemaInference: false,
}

// dataSourceName builds the driver's connection URL.
//
// The password is revealed here and nowhere else. go-ora's BuildUrl escapes the
// components it assembles, so a password containing a URL delimiter does not
// truncate the result. The assembled string goes straight to sql.Open and is
// never logged or returned.
func dataSourceName(cfg dbadapter.ConnectionConfig) string {
	port := defaultPort
	if cfg.Port != nil {
		port = *cfg.Port
	}

	return goora.BuildUrl(
		cfg.Host,
		int(port),
		cfg.Database,
		cfg.Username,
		cfg.Password.RevealString(),
		nil,
	)
}

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
