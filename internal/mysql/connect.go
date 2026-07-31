// Package mysql surveys a MySQL or MariaDB database.
//
// The driver is go-sql-driver/mysql, which is pure Go and therefore satisfies
// R13: no cgo anywhere in the dependency graph, so one static binary reaches an
// airgapped host without a vendor client library present.
package mysql

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strconv"
	"time"

	driver "github.com/go-sql-driver/mysql"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// driverName is what go-sql-driver/mysql registers itself as.
const driverName = "mysql"

// defaultPort is the port MySQL listens on when the configuration names none.
const defaultPort uint16 = 3306

// ErrClosed reports use of an adapter whose pool has been closed.
var ErrClosed = errors.New("mysql: adapter is closed")

// ErrNoDatabase reports a configuration that names no database to survey.
var ErrNoDatabase = errors.New("mysql: no database name was given")

// ErrUnknownTable reports a table the catalog does not have.
var ErrUnknownTable = errors.New("mysql: no such table or view")

// Adapter surveys one MySQL database.
type Adapter struct {
	db           *sql.DB
	host         string
	port         *uint16
	database     string
	username     string
	queryTimeout time.Duration
}

// Adapter satisfies both contracts. The assertions are here rather than in a
// test so that a signature drift is a build failure in this package.
var (
	_ dbadapter.Adapter              = (*Adapter)(nil)
	_ dbadapter.MultiDatabaseAdapter = (*Adapter)(nil)
)

// Open connects to the configured server and verifies the connection is usable.
func Open(ctx context.Context, cfg dbadapter.ConnectionConfig) (*Adapter, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mysql: invalid connection configuration: %w", err)
	}

	if cfg.Database == "" {
		return nil, ErrNoDatabase
	}

	connector, err := driver.NewConnector(dataSourceConfig(cfg))
	if err != nil {
		return nil, fmt.Errorf("mysql: build connector for %s: %w", cfg.Host, err)
	}

	db := sql.OpenDB(connector)
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

// DatabaseType reports MySQL.
func (a *Adapter) DatabaseType() dbschema.DatabaseType {
	return dbschema.MySQL
}

// Supports reports whether MySQL has a capability.
func (a *Adapter) Supports(feature dbadapter.Feature) bool {
	return mysqlFeatures[feature]
}

// Ping verifies the server is reachable and the credential is accepted.
func (a *Adapter) Ping(ctx context.Context) error {
	if a.db == nil {
		return ErrClosed
	}

	if err := a.db.PingContext(ctx); err != nil {
		return fmt.Errorf("mysql: ping %s: %w", a.host, err)
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
		return fmt.Errorf("mysql: close %s: %w", a.host, err)
	}

	return nil
}

// mysqlFeatures records what the engine has. Every Feature is listed, including
// the absent ones, so that adding a capability to the contract makes the
// exhaustiveness check fail here rather than defaulting this engine to "no".
var mysqlFeatures = map[dbadapter.Feature]bool{
	// A MySQL "schema" is a database, so schemas and databases are the same
	// namespace rather than two levels. The survey treats it as the database
	// level, which is what multi-database mode enumerates.
	dbadapter.FeatureSchemas:     false,
	dbadapter.FeatureViews:       true,
	dbadapter.FeatureRoutines:    true,
	dbadapter.FeatureTriggers:    true,
	dbadapter.FeatureCustomTypes: false,
	// TABLE_ROWS is an estimate from the storage engine's statistics, not a
	// count, and it is NULL often enough that the caller must expect it.
	dbadapter.FeatureRowCountEstimate: true,
	dbadapter.FeatureMultiDatabase:    true,
	dbadapter.FeatureSchemaInference:  false,
}

// dataSourceConfig builds the driver configuration for a connection.
//
// The password is revealed here and nowhere else. It goes straight into the
// driver's configuration and is never formatted, logged, or joined into a DSN
// string this code holds: the whole point of the Secret type is that the value
// has exactly one exit, and this is it.
func dataSourceConfig(cfg dbadapter.ConnectionConfig) *driver.Config {
	port := defaultPort
	if cfg.Port != nil {
		port = *cfg.Port
	}

	config := driver.NewConfig()
	config.Net = "tcp"
	config.Addr = cfg.Host + ":" + strconv.FormatUint(uint64(port), 10)
	config.DBName = cfg.Database
	config.User = cfg.Username
	config.Passwd = cfg.Password.RevealString()
	config.Timeout = cfg.ConnectTimeout
	config.ParseTime = true
	config.AllowNativePasswords = true
	config.Params = map[string]string{}

	if cfg.ReadOnly {
		// Unrecognized parameters are sent as session system variables on
		// connect. transaction_read_only makes the server itself refuse a write
		// on this session, which is the second line of defense behind every
		// statement in this package already being a read.
		config.Params["transaction_read_only"] = "1"
	}

	return config
}

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
