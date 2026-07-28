package mongodb

import (
	"context"
	"fmt"
	"slices"
	"time"

	"go.mongodb.org/mongo-driver/v2/bson"
	"go.mongodb.org/mongo-driver/v2/mongo"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// SampleTable reads documents from one collection.
//
// The documents are the only part of a survey that touches user data, so the
// query is as narrow as it can be: one aggregation, bounded by $sample, reading
// nothing else. No temporary collection is created and nothing is written.
func (a *Adapter) SampleTable(
	ctx context.Context,
	table dbadapter.TableRef,
	cfg dbadapter.SamplingConfig,
) (dbschema.TableSample, error) {
	if a.client == nil {
		return dbschema.TableSample{}, ErrClosed
	}

	if err := table.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mongodb: %w", err)
	}

	if err := cfg.Validate(); err != nil {
		return dbschema.TableSample{}, fmt.Errorf("mongodb: invalid sampling configuration: %w", err)
	}

	if err := a.requireCollection(ctx, table.Table); err != nil {
		return dbschema.TableSample{}, err
	}

	rows, err := a.sampleDocuments(ctx, table.Table, cfg)
	if err != nil {
		return dbschema.TableSample{}, err
	}

	status := dbschema.Complete()

	return dbschema.TableSample{
		TableName:  table.Table,
		Rows:       rows,
		SampleSize: sampledRows(rows),
		// $sample draws a random subset rather than the most recent documents.
		// Saying so matters: a reader who assumed "most recent" would read the
		// sample as a view of what the application is doing now.
		SamplingStrategy: dbschema.Random(cfg.SampleSize),
		Ordering:         &dbschema.OrderingStrategy{Kind: dbschema.OrderUnordered},
		CollectedAt:      time.Now(),
		Warnings:         []string{},
		Status:           &status,
	}, nil
}

// requireCollection reports an error when the named collection does not exist.
//
// The aggregation below would return an empty result for a collection that is
// not there, which a caller would read as an empty collection. A misspelled name
// deserves to be told apart from an empty one.
func (a *Adapter) requireCollection(ctx context.Context, name string) error {
	names, err := a.collectionNames(ctx)
	if err != nil {
		return err
	}

	if !slices.Contains(names, name) {
		return fmt.Errorf("%w: %q", ErrUnknownCollection, name)
	}

	return nil
}

// sampleDocuments draws a random subset of the collection's documents.
func (a *Adapter) sampleDocuments(
	ctx context.Context,
	name string,
	cfg dbadapter.SamplingConfig,
) ([]map[string]any, error) {
	timeout := cfg.QueryTimeout
	if timeout <= 0 {
		timeout = a.queryTimeout
	}

	queryCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	pipeline := mongo.Pipeline{
		bson.D{{Key: "$sample", Value: bson.D{{Key: "size", Value: int64(cfg.SampleSize)}}}},
	}

	cursor, err := a.client.Database(a.database).Collection(name).Aggregate(queryCtx, pipeline)
	if err != nil {
		return nil, fmt.Errorf("mongodb: sample %q: %w", name, err)
	}

	defer func() { discardError(cursor.Close(queryCtx)) }()

	rows := make([]map[string]any, 0, cfg.SampleSize)

	for cursor.Next(queryCtx) {
		var document bson.M
		if err := cursor.Decode(&document); err != nil {
			return nil, fmt.Errorf("mongodb: decode a document of %q: %w", name, err)
		}

		rows = append(rows, normalizeDocument(document))
	}

	if err := cursor.Err(); err != nil {
		return nil, fmt.Errorf("mongodb: read the sample of %q: %w", name, err)
	}

	return rows, nil
}

// normalizeDocument converts a decoded document into values that survive a JSON
// round trip as themselves.
//
// The driver decodes into its own BSON types, several of which marshal to JSON
// as a struct of internal fields rather than as the value they represent -- an
// ObjectID becomes an array of bytes, a DateTime becomes a bare integer. Each is
// rendered as the string an operator would recognize instead.
func normalizeDocument(document bson.M) map[string]any {
	normalized := make(map[string]any, len(document))
	for key, value := range document {
		normalized[key] = normalizeValue(value)
	}

	return normalized
}

// normalizeValue renders one decoded BSON value.
func normalizeValue(value any) any {
	switch typed := value.(type) {
	case bson.ObjectID:
		return typed.Hex()
	case bson.DateTime:
		return typed.Time().UTC().Format(time.RFC3339Nano)
	case bson.Decimal128:
		return typed.String()
	case bson.Binary:
		// The subtype is dropped: the bytes are what a reader wants, and
		// encoding/json renders them as base64.
		return typed.Data
	case bson.M:
		return normalizeDocument(typed)
	case bson.A:
		items := make([]any, 0, len(typed))
		for _, item := range typed {
			items = append(items, normalizeValue(item))
		}

		return items
	default:
		return value
	}
}

// sampledRows reports how many documents a sample holds in the width the
// document uses.
//
// The count cannot exceed the limit the query was given, and that limit is
// itself bounded by dbadapter.MaxSampleSize, so the conversion is safe -- but
// the bound is stated here rather than assumed.
func sampledRows(rows []map[string]any) uint32 {
	if len(rows) > int(dbadapter.MaxSampleSize) {
		return dbadapter.MaxSampleSize
	}

	//nolint:gosec // G115: the line above bounds the count by MaxSampleSize.
	return uint32(len(rows))
}
