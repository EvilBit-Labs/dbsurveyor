// Package survey orchestrates one collection run: open an adapter, read the
// schema, optionally sample rows, and write the artifact.
//
// It never imports an adapter package. The set of engines a run may use arrives
// as a Registry from the command layer, which is the concrete form of R11 -- and
// the reason the import graph stays acyclic, since an adapter has to import the
// interface package that this one also depends on. tools/architecture_test.go
// enforces it.
package survey

import (
	"context"
	"fmt"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// Run performs one survey and writes its artifact.
//
// The adapter is closed before the artifact is written, so a slow or failing
// write never holds a connection to somebody else's production database open.
func Run(ctx context.Context, options Options) (Result, error) {
	if err := options.Validate(); err != nil {
		return Result{}, fmt.Errorf("survey: %w", err)
	}

	construct, err := options.constructor()
	if err != nil {
		return Result{}, fmt.Errorf("survey: %w", err)
	}

	started := time.Now()
	report := options.reporter()

	document, err := collect(ctx, construct, options, report)
	if err != nil {
		return Result{}, err
	}

	report.Start(0, "writing the artifact")

	path, err := artifact.Write(options.Output, document, options.Format, options.Password)
	if err != nil {
		return Result{}, fmt.Errorf("survey: write the artifact: %w", err)
	}

	report.Done()

	return Result{
		Path:     path,
		Tables:   len(document.Tables),
		Sampled:  len(document.Samples),
		Objects:  document.ObjectCount(),
		Duration: time.Since(started),
		Warnings: document.CollectionMetadata.Warnings,
	}, nil
}

// collect opens the adapter, reads the schema, and samples what was asked for.
//
// It is separate from Run so the adapter's lifetime is exactly this function:
// the deferred close runs before the artifact is written rather than after.
func collect(
	ctx context.Context,
	construct Constructor,
	options Options,
	report reporterLike,
) (*dbschema.Schema, error) {
	report.Start(0, "connecting")

	adapter, err := construct(ctx, options.Collection.Connection)
	if err != nil {
		// The adapter's error is wrapped rather than replaced, because an
		// adapter's connection errors are written not to carry a DSN. What must
		// not appear here is the target: this package holds a parsed Target
		// whose password is a Secret, and never the string it came from.
		return nil, fmt.Errorf("survey: connect: %w", err)
	}

	defer func() { discardError(adapter.Close()) }()

	if err := adapter.Ping(ctx); err != nil {
		return nil, fmt.Errorf("survey: %w", err)
	}

	report.Done()
	report.Start(0, "collecting the schema")

	document, err := adapter.CollectSchema(ctx, options.Collection)
	if err != nil {
		return nil, fmt.Errorf("survey: collect the schema: %w", err)
	}

	report.Done()

	for _, warning := range document.CollectionMetadata.Warnings {
		report.Warn(warning)
	}

	if options.Collection.Sample {
		sampleTables(ctx, adapter, options, document, report)
	}

	return document, nil
}

// sampleTables reads rows from each table, applies redaction, and attaches the
// samples to the document.
//
// A table that cannot be sampled is recorded as skipped and the survey
// continues. One unreadable table is not a reason to lose the schema, and a
// sample marked skipped is how a reader tells that from an empty table.
func sampleTables(
	ctx context.Context,
	adapter dbadapter.Adapter,
	options Options,
	document *dbschema.Schema,
	report reporterLike,
) {
	tables := selectTables(document, options.MaxTablesSampled)

	report.Start(len(tables), "sampling")

	samples := make([]dbschema.TableSample, 0, len(tables))

	for _, table := range tables {
		if err := throttle(ctx, options.Collection.Sampling.Throttle); err != nil {
			report.Warn("sampling stopped: " + err.Error())

			break
		}

		sample, err := adapter.SampleTable(ctx, table, options.Collection.Sampling)
		if err != nil {
			// The reason names the table and the fact, never the error, which
			// can carry a server address a document must not hold.
			skipped := dbschema.Skipped(fmt.Sprintf("table %s could not be read", table))
			samples = append(samples, dbschema.TableSample{
				TableName:        table.Table,
				Rows:             []map[string]any{},
				SamplingStrategy: dbschema.NoSampling(),
				CollectedAt:      time.Now(),
				Warnings:         []string{},
				Status:           &skipped,
			})

			report.Warn(fmt.Sprintf("table %s could not be sampled", table))
			report.Step(table.String())

			continue
		}

		samples = append(samples, sample)

		for _, warning := range sample.Warnings {
			report.Warn(fmt.Sprintf("%s: %s", table, warning))
		}

		report.Step(table.String())
	}

	// Redaction runs over the whole set once rather than per sample, so a mode
	// change cannot be applied to some tables and not others.
	document.Samples = dbschema.NewRedactor(options.Redaction).Redact(samples)

	report.Done()
}

// selectTables picks the tables to sample, bounded by the configured maximum.
//
// The bound is applied in the document's own table order rather than by any
// notion of importance, so two runs against the same database sample the same
// tables. A limit that silently sampled a different subset each time would make
// two artifacts incomparable.
func selectTables(document *dbschema.Schema, maximum int) []dbadapter.TableRef {
	tables := make([]dbadapter.TableRef, 0, len(document.Tables))

	for i := range document.Tables {
		table := &document.Tables[i]

		reference := dbadapter.TableRef{Table: table.Name}
		if table.Schema != nil {
			reference.Schema = *table.Schema
		}

		tables = append(tables, reference)

		if maximum > 0 && len(tables) == maximum {
			break
		}
	}

	return tables
}

// throttle waits between sampling queries, so a survey does not monopolize a
// database somebody else is using.
//
// It returns the context's error rather than sleeping through a cancellation,
// which is what makes a throttled survey interruptible: a run with a one-second
// throttle over four hundred tables would otherwise ignore a Ctrl-C for as long
// as the current sleep had left.
func throttle(ctx context.Context, delay time.Duration) error {
	if delay <= 0 {
		return ctx.Err()
	}

	timer := time.NewTimer(delay)
	defer timer.Stop()

	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

// reporterLike is the progress surface this package uses. It is declared here
// rather than taking progress.Reporter directly so the functions below can be
// exercised with a recording double.
type reporterLike interface {
	Start(total int, label string)
	Step(label string)
	Warn(message string)
	Done()
}

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
