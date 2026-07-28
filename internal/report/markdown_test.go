package report

import (
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/sebdah/goldie/v2"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbschema"
)

// escapeSequence matches an ANSI control sequence, which must never appear in a
// Markdown report.
var escapeSequence = regexp.MustCompile(`\x1b\[[0-9;]*[a-zA-Z]`)

// TestTheFixtureReportMatchesItsGoldenFile is the stability assertion.
//
// The golden file is Go-authored: it records what this implementation produces
// so that a change to the renderer is a visible diff rather than a surprise in
// somebody's report. It is not a parity corpus from the retired Rust tree, and
// nothing here claims byte compatibility with it.
//
// Run with -update to regenerate after a deliberate change.
func TestTheFixtureReportMatchesItsGoldenFile(t *testing.T) {
	rendered, err := Markdown(newFixture(t), Options{})
	require.NoError(t, err)

	goldie.New(t).Assert(t, "schema", []byte(rendered))
}

// TestTheReportWithSamplesMatchesItsGoldenFile covers the section a default
// report omits.
func TestTheReportWithSamplesMatchesItsGoldenFile(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	rendered, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	goldie.New(t).Assert(t, "schema_with_samples", []byte(rendered))
}

// TestAReportCarriesNoEscapeSequences is the property TERM=dumb has to produce,
// asserted where it is decided: the renderer emits Markdown and nothing else.
// Styling happens in the command layer, over this output, and only when the
// output is a terminal.
func TestAReportCarriesNoEscapeSequences(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	rendered, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	assert.NotRegexp(t, escapeSequence, rendered)
}

// TestAnAbsentSectionIsOmittedRatherThanEmpty is what keeps a report readable.
// A document with no views, routines, triggers, or types is the common case, and
// eleven empty headings is a report nobody scrolls through.
func TestAnAbsentSectionIsOmittedRatherThanEmpty(t *testing.T) {
	document := dbschema.New(
		dbschema.NewDatabaseInfo("empty", dbschema.SQLite),
		"test",
		fixedTime,
	)

	rendered, err := Markdown(document, Options{})
	require.NoError(t, err)

	for _, heading := range []string{
		"## Tables", "## Views", "## Procedures", "## Functions",
		"## Triggers", "## User-defined types", "## Data quality",
		"## Samples", "## Collection warnings",
	} {
		assert.NotContains(t, rendered, heading, "an absent subject gets no heading")
	}

	assert.Contains(t, rendered, "# Database: empty", "the overview is always there")
}

// TestQualityAppearsOnlyWhenAnalysisRan covers the plan's requirement in both
// directions.
func TestQualityAppearsOnlyWhenAnalysisRan(t *testing.T) {
	document := newFixture(t)

	without, err := Markdown(document, Options{})
	require.NoError(t, err)
	assert.NotContains(t, without, "## Data quality")

	document.Samples = newSamples()
	apply(document, Options{Analyze: true, Now: func() time.Time { return fixedTime }})

	with, err := Markdown(document, Options{})
	require.NoError(t, err)
	assert.Contains(t, with, "## Data quality")
	assert.Contains(t, with, "public.users")
}

// TestARowCountThatTheEngineDoesNotMaintainIsNotShownAsZero keeps the two apart:
// an empty table and a table the engine keeps no statistic for are different
// facts, and reporting both as 0 loses one of them.
func TestARowCountThatTheEngineDoesNotMaintainIsNotShownAsZero(t *testing.T) {
	rendered, err := Markdown(newFixture(t), Options{})
	require.NoError(t, err)

	summary := section(t, rendered, "## Tables", "###")

	assert.Contains(t, summary, "| public.orders | 3 | - |", "an absent estimate reads as absent")
	assert.Contains(t, summary, "| public.users | 4 | 1482 |")
}

// TestAPipeInASampledValueDoesNotBreakTheTable is not cosmetic: an unescaped
// pipe ends the cell, so a value containing one renders as different data rather
// than as itself.
func TestAPipeInASampledValueDoesNotBreakTheTable(t *testing.T) {
	status := dbschema.Complete()
	document := newFixture(t)
	document.Samples = []dbschema.TableSample{{
		TableName:        "notes",
		Rows:             []map[string]any{{"body": "before | after", "wrapped": "line one\nline two"}},
		SampleSize:       1,
		SamplingStrategy: dbschema.MostRecent(1),
		CollectedAt:      fixedTime,
		Warnings:         []string{},
		Status:           &status,
	}}

	rendered, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	assert.Contains(t, rendered, `before \| after`, "the pipe is escaped")
	assert.Contains(t, rendered, "line one line two", "the newline is folded")
	assert.NotContains(t, rendered, "line one\nline two")
}

// TestSampleColumnsAreSortedSoAReportIsDiffable records why the order is not the
// row's own: a row is a map, and Go gives no stable iteration order.
func TestSampleColumnsAreSortedSoAReportIsDiffable(t *testing.T) {
	document := newFixture(t)
	document.Samples = newSamples()

	first, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	second, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	assert.Equal(t, first, second, "two renders of one document agree")
	assert.Contains(t, first, "| api_key | email | id |", "columns are sorted")
}

// TestASkippedSampleSaysSoRatherThanLookingEmpty keeps a table nobody could read
// distinguishable from a table with no rows.
func TestASkippedSampleSaysSoRatherThanLookingEmpty(t *testing.T) {
	skipped := dbschema.Skipped("table forbidden could not be read")
	document := newFixture(t)
	document.Samples = []dbschema.TableSample{{
		TableName:        "forbidden",
		Rows:             []map[string]any{},
		SamplingStrategy: dbschema.NoSampling(),
		CollectedAt:      fixedTime,
		Warnings:         []string{},
		Status:           &skipped,
	}}

	rendered, err := Markdown(document, Options{IncludeSamples: true})
	require.NoError(t, err)

	assert.Contains(t, rendered, "Sampling was skipped")
	assert.Contains(t, rendered, "table forbidden could not be read")
}

func TestByteSizesAreRenderedInAReadableUnit(t *testing.T) {
	for size, want := range map[uint64]string{
		0:          "0 B",
		512:        "512 B",
		1024:       "1.0 KiB",
		1536:       "1.5 KiB",
		15_728_640: "15.0 MiB",
		1 << 40:    "1.0 TiB",
	} {
		assert.Equal(t, want, humanBytes(size), "size %d", size)
	}
}

func TestOptionalValuesRenderTheAbsenceMarker(t *testing.T) {
	assert.Equal(t, absent, optional(nil))
	assert.Equal(t, absent, optional(pointer("")))
	assert.Equal(t, absent, optional(pointer("   ")))
	assert.Equal(t, "value", optional(pointer("  value  ")))

	assert.Equal(t, absent, action(nil))
	assert.Equal(t, "cascade", action(pointer(dbschema.Cascade)))

	assert.Equal(t, absent, rowCount(nil))
	assert.Equal(t, "0", rowCount(pointer(uint64(0))))
}

// section returns the part of a report from a heading up to the next heading at
// the given level, so an assertion about one section cannot be satisfied by text
// from another.
func section(t *testing.T, rendered, heading, until string) string {
	t.Helper()

	start := strings.Index(rendered, heading)
	require.GreaterOrEqual(t, start, 0, "no %q section in the report", heading)

	rest := rendered[start+len(heading):]

	if end := strings.Index(rest, "\n"+until); end >= 0 {
		return rest[:end]
	}

	return rest
}
