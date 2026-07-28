package survey

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
	"github.com/EvilBit-Labs/dbsurveyor/internal/progress"
)

// Constructor opens an adapter for a connection.
//
// It is a function rather than an interface so an adapter package needs to know
// nothing about this one: Open already has this shape, and the command layer
// adapts each engine's concrete return type in one line.
type Constructor func(ctx context.Context, cfg dbadapter.ConnectionConfig) (dbadapter.Adapter, error)

// Registry maps a canonical scheme to the adapter that speaks it.
//
// This is the concrete form of R11 and the reason the import graph stays
// acyclic. This package never imports an adapter; the command layer builds the
// map and passes it in, so the set of enabled engines is visible at one call
// site instead of being the side effect of an import somewhere.
type Registry map[string]Constructor

// Options is everything one survey needs.
type Options struct {
	// Target names the server and how to reach it.
	Target Target
	// Adapters is the set of engines this survey may use.
	Adapters Registry
	// Collection governs what is collected.
	Collection dbadapter.CollectionConfig
	// Output is the path to write to, before the format's extension is applied.
	Output string
	// Format selects compression and encryption.
	Format artifact.Format
	// Password supplies the encryption password. It is only called when Format
	// asks for encryption.
	Password artifact.PasswordFunc
	// Redaction masks sampled values before they reach disk.
	Redaction dbschema.RedactionMode
	// Progress reports what the survey is doing. Nil means report nothing.
	Progress progress.Reporter
	// MaxTablesSampled bounds how many tables are sampled. Zero means every one.
	MaxTablesSampled int
}

// Option validation errors.
var (
	ErrNoAdapters = errors.New("no adapters were registered")
	ErrNoOutput   = errors.New("no output path was given")
)

// Validate reports every way the options are unusable.
func (o Options) Validate() error {
	var problems []error

	if o.Target.Scheme == "" {
		problems = append(problems, ErrMissingScheme)
	}

	if len(o.Adapters) == 0 {
		problems = append(problems, ErrNoAdapters)
	}

	if o.Output == "" {
		problems = append(problems, ErrNoOutput)
	}

	problems = append(problems, o.Collection.Validate())

	return errors.Join(problems...)
}

// reporter returns the progress reporter, or one that reports nothing.
func (o Options) reporter() progress.Reporter {
	if o.Progress == nil {
		return progress.Discard
	}

	return o.Progress
}

// constructor looks up the adapter for the target's scheme.
func (o Options) constructor() (Constructor, error) {
	construct, known := o.Adapters[o.Target.Scheme]
	if !known {
		return nil, fmt.Errorf("%w: %q is not registered", ErrUnknownScheme, o.Target.Scheme)
	}

	return construct, nil
}

// Result is what a survey produced.
//
// It names the artifact and counts what went into it. It carries no connection
// detail at all: a caller printing a result must not be able to print a
// credential by accident.
type Result struct {
	// Path is where the artifact was written, including the extension the format
	// implied.
	Path string
	// Tables is how many tables the schema holds.
	Tables int
	// Sampled is how many tables were sampled.
	Sampled int
	// Objects is the total catalog object count.
	Objects int
	// Duration is how long the survey took.
	Duration time.Duration
	// Warnings are the non-fatal problems the survey recorded.
	Warnings []string
}
