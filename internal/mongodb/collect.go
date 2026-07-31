package mongodb

import (
	"context"
	"fmt"
	"time"

	"go.mongodb.org/mongo-driver/v2/bson"
	"go.mongodb.org/mongo-driver/v2/mongo"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// inferenceSampleSize is how many documents inference reads per collection.
//
// It is fixed rather than taken from the sampling configuration, because schema
// inference is not sampling: it runs even when the survey is schema-only, and
// the values it reads never reach the document -- only the field names and the
// types do. A caller who turned sampling off has said they do not want row
// values in the artifact, and this honors that while still producing a schema.
const inferenceSampleSize = 200

// CollectSchema infers the structure of the configured database.
func (a *Adapter) CollectSchema(
	ctx context.Context,
	cfg dbadapter.CollectionConfig,
) (*dbschema.Schema, error) {
	if a.client == nil {
		return nil, ErrClosed
	}

	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mongodb: invalid collection configuration: %w", err)
	}

	started := time.Now()

	info := dbschema.NewDatabaseInfo(a.database, dbschema.MongoDB)
	if version, err := a.version(ctx); err == nil {
		info.Version = &version
	}

	document := dbschema.New(info, cfg.CollectorVersion, started)

	names, err := a.collectionNames(ctx)
	if err != nil {
		return nil, err
	}

	for _, name := range names {
		table, err := a.collectCollection(ctx, name, cfg)
		if err != nil {
			// One unreadable collection is not a reason to lose the rest. The
			// message names the collection and not the error, which can carry a
			// server address a document must never hold.
			document.AddWarning(fmt.Sprintf("collection %q could not be surveyed", name))

			continue
		}

		document.Tables = append(document.Tables, table)
	}

	document.AggregateIndexesAndConstraints()
	document.CollectionMetadata.SetDuration(time.Since(started))

	return document, nil
}

// version reads the server version.
func (a *Adapter) version(ctx context.Context) (string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	var result struct {
		Version string `bson:"version"`
	}

	command := bson.D{{Key: "buildInfo", Value: 1}}
	if err := a.client.Database(a.database).RunCommand(queryCtx, command).Decode(&result); err != nil {
		return "", fmt.Errorf("mongodb: read server version: %w", err)
	}

	return result.Version, nil
}

// collectionNames lists the collections of the database.
func (a *Adapter) collectionNames(ctx context.Context) ([]string, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	names, err := a.client.Database(a.database).ListCollectionNames(queryCtx, bson.D{})
	if err != nil {
		return nil, fmt.Errorf("mongodb: list collections of %q: %w", a.database, err)
	}

	return names, nil
}

// collectCollection infers one collection's shape and reads its indexes.
func (a *Adapter) collectCollection(
	ctx context.Context,
	name string,
	cfg dbadapter.CollectionConfig,
) (dbschema.Table, error) {
	inferred, err := a.inferCollection(ctx, name)
	if err != nil {
		return dbschema.Table{}, err
	}

	table := dbschema.Table{
		Name:        name,
		Columns:     inferred.columns(),
		ForeignKeys: []dbschema.ForeignKey{},
		Indexes:     []dbschema.Index{},
		Constraints: []dbschema.Constraint{},
	}

	// The identifier is the one field MongoDB guarantees, so it is the one
	// primary key a collection has.
	if inferred.fields[identifierField] != nil {
		identifier := identifierField
		table.PrimaryKey = &dbschema.PrimaryKey{Name: &identifier, Columns: []string{identifierField}}
	}

	count, err := a.estimatedCount(ctx, name)
	if err == nil {
		table.RowCount = &count
	}

	if cfg.IncludeIndexes {
		indexes, err := a.collectIndexes(ctx, name)
		if err != nil {
			return dbschema.Table{}, err
		}

		table.Indexes = indexes
	}

	return table, nil
}

// inferCollection samples documents and derives the collection's shape.
//
// $sample is used rather than a plain find with a limit, because a find returns
// documents in storage order and the first N documents of a collection that grew
// over years are the oldest ones -- which is the sample least likely to show the
// fields an application added recently. An empty collection yields an empty
// inference rather than an error: a collection with no documents has no shape,
// and that is a fact rather than a failure.
func (a *Adapter) inferCollection(ctx context.Context, name string) (*inference, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	pipeline := mongo.Pipeline{
		bson.D{{Key: "$sample", Value: bson.D{{Key: "size", Value: inferenceSampleSize}}}},
	}

	cursor, err := a.client.Database(a.database).Collection(name).Aggregate(queryCtx, pipeline)
	if err != nil {
		return nil, fmt.Errorf("mongodb: sample %q for inference: %w", name, err)
	}

	defer func() { discardError(cursor.Close(queryCtx)) }()

	inferred := newInference()

	for cursor.Next(queryCtx) {
		var document bson.M
		if err := cursor.Decode(&document); err != nil {
			return nil, fmt.Errorf("mongodb: decode a document of %q: %w", name, err)
		}

		inferred.observe(document)
	}

	if err := cursor.Err(); err != nil {
		return nil, fmt.Errorf("mongodb: read the sample of %q: %w", name, err)
	}

	return inferred, nil
}

// estimatedCount reads the collection's document count from its metadata.
func (a *Adapter) estimatedCount(ctx context.Context, name string) (uint64, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	count, err := a.client.Database(a.database).Collection(name).EstimatedDocumentCount(queryCtx)
	if err != nil {
		return 0, fmt.Errorf("mongodb: estimate the size of %q: %w", name, err)
	}

	if count < 0 {
		return 0, nil
	}

	return uint64(count), nil
}

// collectIndexes reads a collection's indexes.
//
// The key document is an ordered map of field to direction, and the order is the
// index's key order -- which decides what prefixes it can serve, so it has to be
// read from the ordered form rather than from a decoded map.
func (a *Adapter) collectIndexes(ctx context.Context, name string) ([]dbschema.Index, error) {
	queryCtx, cancel := context.WithTimeout(ctx, a.queryTimeout)
	defer cancel()

	cursor, err := a.client.Database(a.database).Collection(name).Indexes().List(queryCtx)
	if err != nil {
		return nil, fmt.Errorf("mongodb: list indexes of %q: %w", name, err)
	}

	defer func() { discardError(cursor.Close(queryCtx)) }()

	indexes := []dbschema.Index{}

	for cursor.Next(queryCtx) {
		var specification struct {
			Name   string  `bson:"name"`
			Key    bson.D  `bson:"key"`
			Unique *bool   `bson:"unique"`
			Type   *string `bson:"type"`
		}

		if err := cursor.Decode(&specification); err != nil {
			return nil, fmt.Errorf("mongodb: decode an index of %q: %w", name, err)
		}

		indexes = append(indexes, dbschema.Index{
			Name:      specification.Name,
			TableName: name,
			Columns:   indexColumns(specification.Key),
			Unique:    specification.Unique != nil && *specification.Unique,
			// MongoDB names the identifier index _id_ and creates it on every
			// collection; it is the only index that backs the primary key.
			Primary:   specification.Name == identifierIndexName,
			IndexType: specification.Type,
		})
	}

	if err := cursor.Err(); err != nil {
		return nil, fmt.Errorf("mongodb: read the indexes of %q: %w", name, err)
	}

	return indexes, nil
}

// identifierIndexName is the name MongoDB gives the index on _id.
const identifierIndexName = "_id_"

// indexColumns renders an index key document as ordered columns.
//
// A direction is 1 or -1 for an ordinary index and a string -- "text",
// "2dsphere" -- for a special one, where there is no sort order to report.
func indexColumns(key bson.D) []dbschema.IndexColumn {
	columns := make([]dbschema.IndexColumn, 0, len(key))

	for _, element := range key {
		column := dbschema.IndexColumn{Name: element.Key}

		if sorted := sortOrder(element.Value); sorted.ordered {
			column.SortOrder = &sorted.direction
		}

		columns = append(columns, column)
	}

	return columns
}

// indexDirection is how an index key element is sorted, and whether it was a
// sort direction at all. It is one value rather than two booleans, which is the
// shape a caller silently swaps.
type indexDirection struct {
	direction dbschema.SortDirection
	ordered   bool
}

// sortOrder reads an index key element's direction.
//
// A direction is 1 or -1 for an ordinary index and a string -- "text",
// "2dsphere" -- for a special one, where there is no sort order to report.
func sortOrder(value any) indexDirection {
	var magnitude float64

	switch direction := value.(type) {
	case int32:
		magnitude = float64(direction)
	case int64:
		magnitude = float64(direction)
	case float64:
		magnitude = direction
	default:
		return indexDirection{}
	}

	if magnitude < 0 {
		return indexDirection{direction: dbschema.Descending, ordered: true}
	}

	return indexDirection{direction: dbschema.Ascending, ordered: true}
}
