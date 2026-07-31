package postgres

import (
	"context"
	"fmt"
	"slices"
	"time"

	"github.com/jackc/pgx/v5"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// multiDatabasePoolSize caps each per-database pool.
//
// PostgreSQL cannot reach another database from an existing connection, so a
// multi-database survey means one pool per database. Small on purpose: a server
// with fifty databases would otherwise open fifty pools of the default size and
// exhaust max_connections long before the extra connections bought any
// throughput.
const multiDatabasePoolSize uint32 = 2

// ListDatabases enumerates the databases on the server.
//
// The server's own databases are excluded unless the configuration asks for
// them: they are identical on every PostgreSQL server and drown the databases an
// operator is looking at. Names in ExcludeDatabases are dropped either way, and
// a database that refuses connections is never listed at all.
func (a *Adapter) ListDatabases(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) ([]string, error) {
	if a.pool == nil {
		return nil, ErrClosed
	}

	found, err := a.readDatabases(ctx)
	if err != nil {
		return nil, err
	}

	var names []string

	for _, candidate := range found {
		if !cfg.IncludeSystemDatabases && (candidate.template || isSystemDatabase(candidate.name)) {
			continue
		}

		if slices.Contains(cfg.ExcludeDatabases, candidate.name) {
			continue
		}

		names = append(names, candidate.name)
	}

	return names, nil
}

// candidateDatabase is one row of the database listing.
type candidateDatabase struct {
	name     string
	template bool
}

// readDatabases reads every connectable database on the server.
func (a *Adapter) readDatabases(ctx context.Context) ([]candidateDatabase, error) {
	var found []candidateDatabase

	err := a.eachRow(ctx, databasesQuery, nil, func(rows pgx.Rows) error {
		var candidate candidateDatabase
		if err := rows.Scan(&candidate.name, &candidate.template); err != nil {
			return err
		}

		found = append(found, candidate)

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("postgres: list databases on %s: %w", a.host, err)
	}

	return found, nil
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
	if a.pool == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("postgres: invalid collection configuration: %w", err)
	}

	started := time.Now()

	found, err := a.readDatabases(ctx)
	if err != nil {
		return nil, err
	}

	names, err := a.ListDatabases(ctx, cfg)
	if err != nil {
		return nil, err
	}

	version, err := a.version(ctx)
	if err != nil {
		return nil, err
	}

	server := dbschema.NewServerSchema(dbschema.ServerInfo{
		ServerType:              dbschema.PostgreSQL,
		Version:                 version,
		Host:                    a.host,
		Port:                    a.port,
		TotalDatabases:          len(found),
		SystemDatabasesExcluded: len(found) - len(names),
		ConnectionUser:          a.username,
	}, cfg.CollectorVersion, started)

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
		document, err := a.surveyOne(ctx, name, cfg)
		if err != nil {
			tally.failed++

			info := dbschema.NewDatabaseInfo(name, dbschema.PostgreSQL)
			info.AccessLevel = dbschema.AccessNone
			info.SystemDatabase = isSystemDatabase(name)
			// The message is the failure, never the connection that produced it.
			// A wrapped driver error can carry a connection string, and a
			// document carrying one is rejected by the credential scan --
			// correctly, and too late to be useful to the operator who wanted
			// the other databases.
			info.CollectionStatus = dbschema.CollectionFailure(
				fmt.Sprintf("database %q could not be surveyed", name))

			server.Databases = append(server.Databases, *dbschema.New(info, cfg.CollectorVersion, time.Now()))
			server.CollectionMetadata.Warnings = append(server.CollectionMetadata.Warnings,
				fmt.Sprintf("database %q was not collected", name))

			continue
		}

		tally.collected++

		server.Databases = append(server.Databases, *document)
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

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
