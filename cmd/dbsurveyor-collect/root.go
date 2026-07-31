package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"github.com/EvilBit-Labs/dbsurveyor/internal/artifact"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
	"github.com/EvilBit-Labs/dbsurveyor/internal/envelope"
	"github.com/EvilBit-Labs/dbsurveyor/internal/progress"
	"github.com/EvilBit-Labs/dbsurveyor/internal/survey"
)

// connectionEnvVar supplies the connection string when no positional argument
// is given. It is the name every other database tool uses.
const connectionEnvVar = "DATABASE_URL"

// defaultOutput is where a survey is written when no path is given.
const defaultOutput = "schema"

// ErrNoConnectionString reports a run with nothing to connect to.
var ErrNoConnectionString = fmt.Errorf(
	"no connection string: pass one as an argument or set %s", connectionEnvVar)

// options are the flag values one run was given.
//
// The connection string is deliberately absent: it is read once inside RunE,
// parsed immediately, and never stored. A struct field holding it would be a
// field something could print.
type options struct {
	output    string
	compress  bool
	encrypt   bool
	sample    bool
	sampleSiz uint32
	throttle  time.Duration
	redaction string
	maxTables int
	timeout   time.Duration

	includeViews    bool
	includeRoutines bool
	includeTriggers bool
	includeIndexes  bool
	includeTypes    bool

	quiet bool
}

// newRootCommand builds the collector's command.
//
// This file parses flags and wires; it holds no collection logic (R9). Every
// decision about what a survey does lives in internal/survey, which is where it
// can be tested without a process.
func newRootCommand(version string) *cobra.Command {
	opts := &options{}

	command := &cobra.Command{
		Use:   "dbsurveyor-collect [connection-string]",
		Short: "Survey a database and write a portable schema artifact",
		Long: strings.TrimSpace(`
Survey a database server and write its structure to a portable artifact.

Every database operation is a read. The tool makes no network call except to the
database it was pointed at, and writes nothing anywhere but the output path.

The connection string may be given as an argument or in ` + connectionEnvVar + `.
An argument wins when both are present. Supported schemes:

  ` + strings.Join(survey.SupportedSchemes(), ", ") + `

An encryption password is read from ` + envelope.PasswordEnvVar + ` when set, and
prompted for otherwise.`),
		Args:          cobra.MaximumNArgs(1),
		Version:       version,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(command *cobra.Command, args []string) error {
			return run(command, args, opts, version)
		},
	}

	bindFlags(command, opts)

	// Compression inside an envelope is decided by the artifact package, which
	// always compresses before encrypting; asking for both here would suggest
	// the two are independent choices.
	command.MarkFlagsMutuallyExclusive("compress", "encrypt")

	return command
}

// bindFlags declares the flag surface.
func bindFlags(command *cobra.Command, opts *options) {
	flags := command.Flags()

	flags.StringVarP(&opts.output, "output", "o", defaultOutput,
		"path to write the artifact to, without an extension")
	flags.BoolVar(&opts.compress, "compress", false, "compress the artifact with zstd")
	flags.BoolVar(&opts.encrypt, "encrypt", false,
		"encrypt the artifact; the password comes from "+envelope.PasswordEnvVar+" or a prompt")

	flags.BoolVar(&opts.sample, "sample", false,
		"read rows from each table; off by default because it is the only part that reads user data")
	flags.Uint32Var(&opts.sampleSiz, "sample-size", dbadapter.DefaultSampleSize,
		"rows to read per table when sampling")
	flags.DurationVar(&opts.throttle, "throttle", 0,
		"delay between sampling queries, to keep a survey from monopolizing a database")
	flags.IntVar(&opts.maxTables, "max-tables-sampled", 0,
		"stop sampling after this many tables; 0 means every table")
	flags.StringVar(&opts.redaction, "redact-mode", string(dbschema.RedactBalanced),
		"how aggressively to mask sampled values: none, minimal, balanced, conservative")

	flags.DurationVar(&opts.timeout, "timeout", 0,
		"abandon the survey after this long; 0 means no limit")

	flags.BoolVar(&opts.includeViews, "views", true, "collect views")
	flags.BoolVar(&opts.includeRoutines, "routines", true, "collect procedures and functions")
	flags.BoolVar(&opts.includeTriggers, "triggers", true, "collect triggers")
	flags.BoolVar(&opts.includeIndexes, "indexes", true, "collect indexes")
	flags.BoolVar(&opts.includeTypes, "types", true, "collect user-defined types")

	flags.BoolVarP(&opts.quiet, "quiet", "q", false, "suppress progress output")
}

// run resolves the connection string, builds the survey options, and reports the
// result.
func run(command *cobra.Command, args []string, opts *options, version string) error {
	target, err := resolveTarget(args)
	if err != nil {
		return err
	}

	ctx := command.Context()
	if ctx == nil {
		ctx = context.Background()
	}

	if opts.timeout > 0 {
		var cancel context.CancelFunc

		ctx, cancel = context.WithTimeout(ctx, opts.timeout)
		defer cancel()
	}

	surveyOptions, err := opts.surveyOptions(target, version, command.OutOrStdout())
	if err != nil {
		return err
	}

	result, err := survey.Run(ctx, surveyOptions)
	if err != nil {
		return err
	}

	report(command.OutOrStdout(), result)

	return nil
}

// resolveTarget reads the connection string and parses it.
//
// The raw string exists only inside this function. It is not logged, not stored
// on the options, and not included in the error when parsing fails -- the parse
// errors are written to describe the problem without echoing the input, because
// the input carries a password.
func resolveTarget(args []string) (survey.Target, error) {
	raw := os.Getenv(connectionEnvVar)
	if len(args) > 0 && strings.TrimSpace(args[0]) != "" {
		// An argument wins over the environment: an operator who typed one meant
		// it, and a stale DATABASE_URL in a shell is a common way to survey the
		// wrong database.
		raw = args[0]
	}

	if strings.TrimSpace(raw) == "" {
		return survey.Target{}, ErrNoConnectionString
	}

	target, err := survey.ParseTarget(raw)
	if err != nil {
		return survey.Target{}, err
	}

	return target, nil
}

// surveyOptions turns the flag values into what the survey package needs.
func (o *options) surveyOptions(
	target survey.Target,
	version string,
	out io.Writer,
) (survey.Options, error) {
	mode := dbschema.RedactionMode(strings.ToLower(strings.TrimSpace(o.redaction)))
	if !mode.Valid() {
		return survey.Options{}, fmt.Errorf(
			"unknown redact-mode %q: choose one of none, minimal, balanced, conservative",
			o.redaction,
		)
	}

	collection := dbadapter.CollectionConfig{
		Connection:           target.Connection,
		Sampling:             dbadapter.NewSamplingConfig(o.sampleSiz),
		Sample:               o.sample,
		IncludeViews:         o.includeViews,
		IncludeRoutines:      o.includeRoutines,
		IncludeTriggers:      o.includeTriggers,
		IncludeIndexes:       o.includeIndexes,
		IncludeConstraint:    true,
		IncludeTypes:         o.includeTypes,
		MaxConcurrentQueries: dbadapter.DefaultMaxConnections,
		CollectorVersion:     version,
	}
	collection.Sampling.Throttle = o.throttle

	return survey.Options{
		Target:           target,
		Adapters:         adapters(),
		Collection:       collection,
		Output:           o.output,
		Format:           artifact.Format{Compress: o.compress, Encrypt: o.encrypt},
		Password:         envelope.NewPasswordReader().ForSeal,
		Redaction:        mode,
		Progress:         o.reporter(out),
		MaxTablesSampled: o.maxTables,
	}, nil
}

// reporter builds the progress reporter, honoring --quiet.
func (o *options) reporter(out io.Writer) progress.Reporter {
	if o.quiet {
		return progress.Discard
	}

	return progress.New(out)
}

// report prints what the survey produced.
//
// It prints the artifact path and counts and nothing about the connection. A
// summary line naming the server is the sort of convenience that eventually
// grows a username and then a password.
func report(out io.Writer, result survey.Result) {
	discardError(fmt.Fprintf(out, "wrote %s\n", result.Path))
	discardError(fmt.Fprintf(out, "  %d tables, %d objects", result.Tables, result.Objects))

	if result.Sampled > 0 {
		discardError(fmt.Fprintf(out, ", %d sampled", result.Sampled))
	}

	discardError(fmt.Fprintf(out, ", in %s\n", result.Duration.Round(time.Millisecond)))

	if len(result.Warnings) > 0 {
		discardError(fmt.Fprintf(out, "  %d warnings are recorded in the artifact\n", len(result.Warnings)))
	}
}

// exitCode maps an error to a process exit status.
//
// A usage mistake and a failed survey are told apart, because a script wrapping
// this tool needs to know whether to fix its arguments or its database.
func exitCode(err error) int {
	switch {
	case err == nil:
		return 0
	case errors.Is(err, ErrNoConnectionString),
		errors.Is(err, survey.ErrEmptyTarget),
		errors.Is(err, survey.ErrMalformedTarget),
		errors.Is(err, survey.ErrMissingScheme),
		errors.Is(err, survey.ErrMissingHost),
		errors.Is(err, survey.ErrUnknownScheme):
		return usageExitCode
	default:
		return failureExitCode
	}
}

// The process exit statuses.
const (
	usageExitCode   = 2
	failureExitCode = 1
)

// discardError drops an error that carries no information a caller can act on.
func discardError(int, error) {}
