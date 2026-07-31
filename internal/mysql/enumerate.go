package mysql

import (
	"context"
	"database/sql"
	"fmt"
	"slices"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// databasesQuery lists the databases the connecting credential can see. A
// credential without privileges on a database does not see it here at all, so
// the result is already scoped to what a survey could read.
const databasesQuery = `SELECT SCHEMA_NAME FROM INFORMATION_SCHEMA.SCHEMATA ORDER BY SCHEMA_NAME`

// multiDatabasePoolSize caps each per-database pool.
//
// Small on purpose: a server with fifty databases would otherwise open fifty
// pools of the default size and exhaust max_connections long before the extra
// connections bought any throughput.
const multiDatabasePoolSize uint32 = 2

// ListDatabases enumerates the databases on the server.
//
// The server's own catalogs are excluded unless the configuration asks for them:
// they are identical on every MySQL server and drown the databases an operator
// is looking at. Names in ExcludeDatabases are dropped either way.
func (a *Adapter) ListDatabases(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) ([]string, error) {
	if a.db == nil {
		return nil, ErrClosed
	}

	var names []string

	err := a.eachRow(ctx, databasesQuery, nil, func(rows *sql.Rows) error {
		var name string
		if err := rows.Scan(&name); err != nil {
			return err
		}

		if !cfg.IncludeSystemDatabases && isSystemDatabase(name) {
			return nil
		}

		if slices.Contains(cfg.ExcludeDatabases, name) {
			return nil
		}

		names = append(names, name)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("mysql: list databases on %s: %w", a.host, err)
	}

	return names, nil
}

// CollectServerSchema surveys every database the configuration selects.
//
// Each database gets its own adapter and its own small pool, closed as soon as
// that database is done rather than at the end of the run. A database that
// cannot be read is recorded with a failure status and the survey continues: one
// unreadable database is not a reason to lose the other forty.
func (a *Adapter) CollectServerSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.ServerSchema, error) {
	if a.db == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mysql: invalid collection configuration: %w", err)
	}

	started := time.Now()

	names, err := a.ListDatabases(ctx, cfg)
	if err != nil {
		return nil, err
	}

	discovered, err := a.countDatabases(ctx)
	if err != nil {
		return nil, err
	}

	version, err := a.version(ctx)
	if err != nil {
		return nil, err
	}

	info := dbschema.ServerInfo{
		ServerType:              dbschema.MySQL,
		Version:                 version,
		Host:                    a.host,
		Port:                    a.port,
		TotalDatabases:          discovered,
		SystemDatabasesExcluded: discovered - len(names),
		ConnectionUser:          a.username,
	}

	server := dbschema.NewServerSchema(info, cfg.CollectorVersion, started)

	tally := a.surveyEach(ctx, names, cfg, server)

	server.ServerInfo.CollectedDatabases = tally.collected
	server.ServerInfo.CollectionMode = dbschema.MultiDatabase(len(names), tally.collected, tally.failed)
	server.CollectionMetadata.SetDuration(time.Since(started))

	return server, nil
}

// surveyTally is how a server survey fared, returned as one value rather than as
// two ints: a pair of same-typed results is the shape a caller silently swaps.
type surveyTally struct {
	collected int
	failed    int
}

// surveyEach collects each named database in turn, reporting how many succeeded
// and how many failed.
func (a *Adapter) surveyEach(
	ctx context.Context,
	names []string,
	cfg dbadapter.CollectionConfig,
	server *dbschema.ServerSchema,
) surveyTally {
	var tally surveyTally

	for _, name := range names {
		schema, err := a.surveyOne(ctx, name, cfg)
		if err != nil {
			tally.failed++

			info := dbschema.NewDatabaseInfo(name, dbschema.MySQL)
			info.AccessLevel = dbschema.AccessNone
			info.SystemDatabase = isSystemDatabase(name)
			// The message is the failure, never the connection that produced it.
			// A wrapped driver error can carry a DSN, and a document carrying one
			// is rejected by the credential scan -- correctly, and too late to be
			// useful to the operator who wanted the other databases.
			info.CollectionStatus = dbschema.CollectionFailure(
				fmt.Sprintf("database %q could not be surveyed", name))

			server.Databases = append(server.Databases, *dbschema.New(info, cfg.CollectorVersion, time.Now()))
			server.CollectionMetadata.Warnings = append(server.CollectionMetadata.Warnings,
				fmt.Sprintf("database %q was not collected", name))

			continue
		}

		tally.collected++

		server.Databases = append(server.Databases, *schema)
	}

	return tally
}

// surveyOne opens a dedicated adapter for one database, collects it, and closes
// the pool before returning.
func (a *Adapter) surveyOne(
	ctx context.Context,
	name string,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	connection := cfg.Connection
	connection.Host = a.host
	connection.Port = a.port
	connection.Database = name
	connection.MaxConnections = multiDatabasePoolSize
	connection.MinIdleConnections = 0

	adapter, err := Open(ctx, connection)
	if err != nil {
		return nil, err
	}

	// The pool is closed here rather than at the end of the run, so a server with
	// many databases never holds more than one database's worth of connections.
	defer func() { discardError(adapter.Close()) }()

	scoped := cfg
	scoped.Connection = connection

	return adapter.CollectSchema(ctx, scoped)
}

// countDatabases counts every database on the server, including the ones a
// survey excludes, so the document can report what it chose not to collect.
func (a *Adapter) countDatabases(ctx context.Context) (int, error) {
	total := 0

	err := a.eachRow(ctx, databasesQuery, nil, func(rows *sql.Rows) error {
		var name string
		if err := rows.Scan(&name); err != nil {
			return err
		}

		total++

		return nil
	})
	if err != nil {
		return 0, fmt.Errorf("mysql: count databases on %s: %w", a.host, err)
	}

	return total, nil
}
