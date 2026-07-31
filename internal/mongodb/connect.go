// Package mongodb surveys a MongoDB database.
//
// The driver is the official go.mongodb.org/mongo-driver, which is pure Go and
// therefore satisfies R13.
//
// MongoDB stores no schema, so there is nothing to read out of a catalog. The
// shape reported here is inferred from a sample of documents, which makes every
// column in the resulting document a claim about the sample rather than about
// the collection. infer.go is where that distinction is kept honest.
package mongodb

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"time"

	"go.mongodb.org/mongo-driver/v2/mongo"
	"go.mongodb.org/mongo-driver/v2/mongo/options"
	"go.mongodb.org/mongo-driver/v2/mongo/readpref"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// defaultPort is the port mongod listens on when the configuration names none.
const defaultPort uint16 = 27017

// ErrClosed reports use of an adapter whose client has been disconnected.
var ErrClosed = errors.New("mongodb: adapter is closed")

// ErrNoDatabase reports a configuration that names no database to survey.
var ErrNoDatabase = errors.New("mongodb: no database name was given")

// ErrUnknownCollection reports a collection the database does not have.
var ErrUnknownCollection = errors.New("mongodb: no such collection")

// Adapter surveys one MongoDB database.
//
// A collection maps to a table and an inferred field to a column, so a MongoDB
// survey produces the same document shape as a relational one. Where that
// mapping is lossy the loss is recorded on the column rather than hidden.
type Adapter struct {
	client       *mongo.Client
	host         string
	port         *uint16
	database     string
	username     string
	queryTimeout time.Duration
}

// Adapter satisfies the adapter contract. The assertion is here rather than in a
// test so that a signature drift is a build failure in this package.
var _ dbadapter.Adapter = (*Adapter)(nil)

// Open connects to the configured server and verifies the connection is usable.
func Open(ctx context.Context, cfg dbadapter.ConnectionConfig) (*Adapter, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("mongodb: invalid connection configuration: %w", err)
	}

	if cfg.Database == "" {
		return nil, ErrNoDatabase
	}

	client, err := mongo.Connect(clientOptions(cfg))
	if err != nil {
		return nil, fmt.Errorf("mongodb: connect to %s: %w", cfg.Host, err)
	}

	adapter := &Adapter{
		client:       client,
		host:         cfg.Host,
		port:         cfg.Port,
		database:     cfg.Database,
		username:     cfg.Username,
		queryTimeout: cfg.QueryTimeout,
	}

	connect, cancel := context.WithTimeout(ctx, cfg.ConnectTimeout)
	defer cancel()

	if err := adapter.Ping(connect); err != nil {
		// The client never became usable, so the disconnect error says nothing
		// an operator can act on and the connect error is the one they need.
		discardError(client.Disconnect(context.WithoutCancel(connect)))

		return nil, err
	}

	return adapter, nil
}

// DatabaseType reports MongoDB.
func (a *Adapter) DatabaseType() dbschema.DatabaseType {
	return dbschema.MongoDB
}

// Supports reports whether MongoDB has a capability.
func (a *Adapter) Supports(feature dbadapter.Feature) bool {
	return mongoFeatures[feature]
}

// Ping verifies the server is reachable and the credential is accepted.
func (a *Adapter) Ping(ctx context.Context) error {
	if a.client == nil {
		return ErrClosed
	}

	if err := a.client.Ping(ctx, readpref.Primary()); err != nil {
		return fmt.Errorf("mongodb: ping %s: %w", a.host, err)
	}

	return nil
}

// Close disconnects the client. It is safe to call more than once.
func (a *Adapter) Close() error {
	if a.client == nil {
		return nil
	}

	client := a.client
	a.client = nil

	if err := client.Disconnect(context.Background()); err != nil {
		return fmt.Errorf("mongodb: disconnect %s: %w", a.host, err)
	}

	return nil
}

// mongoFeatures records what the engine has. Every Feature is listed, including
// the absent ones, so that adding a capability to the contract makes the
// exhaustiveness check fail here rather than defaulting this engine to "no".
var mongoFeatures = map[dbadapter.Feature]bool{
	// A MongoDB database holds collections directly; there is no namespace
	// between the two.
	dbadapter.FeatureSchemas: false,
	// A view exists but is a stored aggregation pipeline rather than a query
	// with a column list, and is reported as a collection rather than modelled
	// separately.
	dbadapter.FeatureViews:       false,
	dbadapter.FeatureRoutines:    false,
	dbadapter.FeatureTriggers:    false,
	dbadapter.FeatureCustomTypes: false,
	// estimatedDocumentCount reads collection metadata rather than counting.
	dbadapter.FeatureRowCountEstimate: true,
	dbadapter.FeatureMultiDatabase:    false,
	// The reason this adapter exists in the shape it does.
	dbadapter.FeatureSchemaInference: true,
}

// clientOptions builds the driver's client configuration.
//
// The password is revealed here and nowhere else. It goes into the URI through
// url.UserPassword, which escapes it, so a password containing a delimiter does
// not truncate the URI. The assembled URI goes straight to the driver and is
// never logged or returned.
func clientOptions(cfg dbadapter.ConnectionConfig) *options.ClientOptions {
	port := defaultPort
	if cfg.Port != nil {
		port = *cfg.Port
	}

	uri := url.URL{
		Scheme: "mongodb",
		Host:   cfg.Host + ":" + strconv.FormatUint(uint64(port), 10),
		Path:   "/" + cfg.Database,
	}

	if cfg.Username != "" {
		uri.User = url.UserPassword(cfg.Username, cfg.Password.RevealString())
	}

	client := options.Client().ApplyURI(uri.String())
	client.SetConnectTimeout(cfg.ConnectTimeout)
	client.SetMaxPoolSize(uint64(cfg.MaxConnections))
	client.SetMinPoolSize(uint64(cfg.MinIdleConnections))
	client.SetMaxConnIdleTime(cfg.MaxIdleTime)

	if cfg.ReadOnly {
		// MongoDB has no read-only session mode. A read preference is the
		// nearest thing: pointing at a secondary means the connection reaches a
		// member that rejects writes outright. On a standalone server it has no
		// effect, so unlike PostgreSQL or SQLite the read-only guarantee here
		// rests on this package issuing only reads. See GOTCHAS.
		client.SetReadPreference(readpref.SecondaryPreferred())
	}

	return client
}

// discardError drops an error that carries no information a caller can act on.
// It is a named function rather than an assignment to the blank identifier so
// that the drop is visible in review and survives errcheck's check-blank.
func discardError(error) {}
