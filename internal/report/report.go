// Package report turns a schema artifact into something a person reads.
//
// Loading always goes through internal/artifact, so the recursive credential
// scan happens before any of this package sees a document. That ordering is the
// point: a report is the thing most likely to be pasted into a ticket or a chat
// window, and a credential that survived to rendering has already escaped.
package report

import (
	"errors"
	"fmt"
	"io"
	"time"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// Options govern how a report is produced.
type Options struct {
	// Redaction masks sampled values before rendering. It applies on top of
	// whatever the collector already applied: redaction is idempotent, so
	// re-applying it cannot un-redact anything.
	Redaction dbschema.RedactionMode
	// Analyze runs quality analysis over the samples the artifact carries.
	Analyze bool
	// Quality configures the analysis. The zero value means the defaults.
	Quality *dbschema.QualityConfig
	// IncludeSamples renders sampled rows. Off by default: the schema is what a
	// report is usually for, and rows are the part that carries user data.
	IncludeSamples bool
	// Password supplies the decryption password for an encrypted artifact.
	Password artifact.PasswordFunc
	// Now supplies the analysis timestamp. Nil means the wall clock; a test
	// supplies its own so a golden file does not change every second.
	Now func() time.Time
}

// ErrNoInput reports a request with no artifact to read.
var ErrNoInput = errors.New("no artifact path was given")

// Load reads an artifact and applies the report-time transformations.
//
// The path's extension decides how it is read -- plain, compressed, encrypted --
// and internal/artifact scans the bytes for credentials before decoding and
// validates after. A document that reaches this function has been through both.
func Load(path string, options Options) (*dbschema.Schema, error) {
	if path == "" {
		return nil, ErrNoInput
	}

	document, err := artifact.ReadSchema(path, options.Password)
	if err != nil {
		// The path is named because an operator running over a directory of
		// artifacts needs to know which one failed. It is a path they supplied,
		// not a credential.
		return nil, fmt.Errorf("read %s: %w", path, err)
	}

	apply(document, options)

	return document, nil
}

// apply performs the report-time transformations on a loaded document.
func apply(document *dbschema.Schema, options Options) {
	if len(document.Samples) > 0 && options.Redaction != "" {
		document.Samples = dbschema.NewRedactor(options.Redaction).Redact(document.Samples)
	}

	if options.Analyze && len(document.Samples) > 0 {
		document.QualityMetrics = analyzer(options).AnalyzeAll(document.Samples, options.clock()())
	}

	if !options.IncludeSamples {
		// The samples are dropped rather than left for the renderer to skip, so
		// a future renderer cannot accidentally start printing them.
		document.Samples = nil
	}
}

// analyzer builds the quality analyzer from the options.
func analyzer(options Options) *dbschema.Analyzer {
	if options.Quality != nil {
		return dbschema.NewAnalyzer(*options.Quality)
	}

	return dbschema.NewAnalyzer(dbschema.DefaultQualityConfig())
}

// clock returns the time source, defaulting to the wall clock.
func (o Options) clock() func() time.Time {
	if o.Now != nil {
		return o.Now
	}

	return time.Now
}

// Render writes a report for the document to out.
func Render(out io.Writer, document *dbschema.Schema, options Options) error {
	rendered, err := Markdown(document, options)
	if err != nil {
		return err
	}

	if _, err := io.WriteString(out, rendered); err != nil {
		return fmt.Errorf("write the report: %w", err)
	}

	return nil
}
