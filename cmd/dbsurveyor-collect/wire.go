package main

import (
	"context"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/mongodb"
	"github.com/EvilBit-Labs/dbsurveyor/internal/mssql"
	"github.com/EvilBit-Labs/dbsurveyor/internal/mysql"
	"github.com/EvilBit-Labs/dbsurveyor/internal/oracle"
	"github.com/EvilBit-Labs/dbsurveyor/internal/postgres"
	"github.com/EvilBit-Labs/dbsurveyor/internal/sqlite"
	"github.com/EvilBit-Labs/dbsurveyor/internal/survey"
)

// adapters is the set of engines this binary can survey.
//
// This is the only file in the tree that imports an adapter package, which is
// what R11 asks for: construction is explicit wiring at the command layer rather
// than registration through init. A registry built by import side effect would
// make the set of enabled engines invisible at the call site and link every
// driver into every binary whether or not it is reachable.
//
// Each entry is a one-line adaptation, because each adapter's Open already has
// the shape survey.Constructor wants; the wrapper exists only because Go will
// not convert a func returning *postgres.Adapter to one returning
// dbadapter.Adapter.
func adapters() survey.Registry {
	return survey.Registry{
		survey.SchemePostgres:  open(postgres.Open),
		survey.SchemeMySQL:     open(mysql.Open),
		survey.SchemeSQLite:    open(sqlite.Open),
		survey.SchemeMongoDB:   open(mongodb.Open),
		survey.SchemeSQLServer: open(mssql.Open),
		survey.SchemeOracle:    open(oracle.Open),
	}
}

// open adapts an adapter's concrete constructor to the interface one.
//
// The nil check matters: a constructor that returns a typed nil pointer and no
// error would produce a non-nil interface holding a nil pointer, and the survey
// would call a method on it rather than reporting the failure.
func open[T dbadapter.Adapter](
	construct func(context.Context, dbadapter.ConnectionConfig) (T, error),
) survey.Constructor {
	return func(ctx context.Context, cfg dbadapter.ConnectionConfig) (dbadapter.Adapter, error) {
		adapter, err := construct(ctx, cfg)
		if err != nil {
			return nil, err
		}

		return adapter, nil
	}
}
