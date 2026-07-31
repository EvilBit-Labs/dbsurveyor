package survey

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestEveryEngineIsReachableByItsOwnSpelling(t *testing.T) {
	for scheme, want := range map[string]string{
		"postgres":      SchemePostgres,
		"postgresql":    SchemePostgres,
		"mysql":         SchemeMySQL,
		"mariadb":       SchemeMySQL,
		"mongodb":       SchemeMongoDB,
		"mongodb+srv":   SchemeMongoDB,
		"sqlserver":     SchemeSQLServer,
		"mssql":         SchemeSQLServer,
		"oracle":        SchemeOracle,
		"oracle+go-ora": SchemeOracle,
	} {
		t.Run(scheme, func(t *testing.T) {
			target, err := ParseTarget(scheme + "://db.internal/shop")
			require.NoError(t, err)
			assert.Equal(t, want, target.Scheme)
		})
	}
}

func TestASchemeIsMatchedWithoutRegardToCase(t *testing.T) {
	target, err := ParseTarget("POSTGRES://db.internal/shop")
	require.NoError(t, err)
	assert.Equal(t, SchemePostgres, target.Scheme)
}

func TestAServerTargetCarriesItsHostPortDatabaseAndUser(t *testing.T) {
	target, err := ParseTarget("postgres://surveyor:secret@db.internal:6543/shop")
	require.NoError(t, err)

	assert.Equal(t, SchemePostgres, target.Scheme)
	assert.Equal(t, "db.internal", target.Connection.Host)
	require.NotNil(t, target.Connection.Port)
	assert.Equal(t, uint16(6543), *target.Connection.Port)
	assert.Equal(t, "shop", target.Connection.Database)
	assert.Equal(t, "surveyor", target.Connection.Username)
	assert.Equal(t, len("secret"), target.Connection.Password.Len())
	assert.True(t, target.Connection.ReadOnly, "read-only is the default, not an opt-in")
}

func TestAnAbsentPortMeansTheEngineDefault(t *testing.T) {
	target, err := ParseTarget("mysql://db.internal/shop")
	require.NoError(t, err)

	assert.Nil(t, target.Connection.Port, "an absent port is not a failure")
}

// TestSQLServerNamesItsDatabaseInTheQueryString covers the form that engine's
// own tooling emits.
func TestSQLServerNamesItsDatabaseInTheQueryString(t *testing.T) {
	target, err := ParseTarget("sqlserver://sa:secret@db.internal:1433?database=shop")
	require.NoError(t, err)

	assert.Equal(t, SchemeSQLServer, target.Scheme)
	assert.Equal(t, "shop", target.Connection.Database)
}

// TestASQLiteTargetCarriesAPathRatherThanAHost covers both spellings an operator
// is likely to write.
func TestASQLiteTargetCarriesAPathRatherThanAHost(t *testing.T) {
	for name, spelling := range map[string]struct {
		raw  string
		path string
	}{
		"absolute":   {"sqlite:///var/lib/app/schema.db", "/var/lib/app/schema.db"},
		"relative":   {"sqlite://./schema.db", "./schema.db"},
		"opaque":     {"sqlite:schema.db", "schema.db"},
		"file alias": {"file:///var/lib/app/schema.db", "/var/lib/app/schema.db"},
	} {
		t.Run(name, func(t *testing.T) {
			target, err := ParseTarget(spelling.raw)
			require.NoError(t, err)

			assert.Equal(t, SchemeSQLite, target.Scheme)
			assert.Equal(t, spelling.path, target.Connection.Host)
		})
	}
}

func TestAnUnknownSchemeNamesTheSupportedOnes(t *testing.T) {
	_, err := ParseTarget("cassandra://db.internal/shop")

	require.ErrorIs(t, err, ErrUnknownScheme)
	assert.Contains(t, err.Error(), "cassandra")
	assert.Contains(t, err.Error(), "postgres", "the message lists what would have worked")
	assert.Contains(t, err.Error(), "mongodb")
}

func TestAnUnusableTargetIsRejected(t *testing.T) {
	for name, want := range map[string]struct {
		raw string
		err error
	}{
		"empty":          {"", ErrEmptyTarget},
		"whitespace":     {"   ", ErrEmptyTarget},
		"no scheme":      {"db.internal/shop", ErrMissingScheme},
		"no host":        {"postgres:///shop", ErrMissingHost},
		"no sqlite path": {"sqlite://", ErrMissingHost},
	} {
		t.Run(name, func(t *testing.T) {
			_, err := ParseTarget(want.raw)
			require.ErrorIs(t, err, want.err)
		})
	}
}

// TestAParseFailureNeverEchoesTheConnectionString is the reason these errors are
// terse. net/url's own parse error quotes the URL it failed on, and for a
// connection string that means the password -- so wrapping one would put a
// credential into an error a caller prints.
func TestAParseFailureNeverEchoesTheConnectionString(t *testing.T) {
	const password = "hunter2-the-actual-password"

	// A control character makes url.Parse fail, and its error would quote the
	// whole string including the userinfo.
	raw := "postgres://surveyor:" + password + "@db.internal\x7f/shop"

	_, err := ParseTarget(raw)

	require.ErrorIs(t, err, ErrMalformedTarget)
	assert.NotContains(t, err.Error(), password)
	assert.NotContains(t, err.Error(), "surveyor")
}

// TestAParsedTargetDoesNotFormatItsPassword keeps the property once parsing has
// succeeded, which is the ordinary case.
func TestAParsedTargetDoesNotFormatItsPassword(t *testing.T) {
	const password = "hunter2-the-actual-password"

	target, err := ParseTarget("mysql://surveyor:" + password + "@db.internal/shop")
	require.NoError(t, err)

	for _, verb := range []string{"%v", "%+v", "%#v", "%s"} {
		rendered := fmt.Sprintf(verb, target)
		assert.NotContains(t, rendered, password, "verb %s", verb)
	}
}

func TestSupportedSchemesIsSortedAndComplete(t *testing.T) {
	schemes := SupportedSchemes()

	assert.Len(t, schemes, len(schemeAliases))
	assert.IsIncreasing(t, schemes, "sorted, so the error message does not reorder between runs")
	assert.Contains(t, schemes, "postgresql")
	assert.Contains(t, schemes, "sqlite")
}

// TestSQLiteTargetRefusesUserinfoInTheOpaqueForm pins GOTCHAS section 7.3 at the
// one branch that reads unparsed text from the URL.
//
// Without the "//" there is no authority for net/url to split, so the userinfo
// of a mistyped sqlite:user:pass@file.db stays whole in Opaque. It used to
// become the file path, which the adapter then quoted verbatim in the stat error
// it returned -- putting a credential-shaped string on stderr.
func TestSQLiteTargetRefusesUserinfoInTheOpaqueForm(t *testing.T) {
	t.Parallel()

	_, err := ParseTarget("sqlite:operator:hunter2@notes.db")
	require.ErrorIs(t, err, ErrMalformedTarget)
	assert.NotContains(t, err.Error(), "hunter2", "the error must not quote what it refused")
}

// TestSQLiteTargetKeepsOrdinaryOpaquePaths guards the refusal above from
// swallowing the relative-path spelling it shares a branch with.
func TestSQLiteTargetKeepsOrdinaryOpaquePaths(t *testing.T) {
	t.Parallel()

	for name, raw := range map[string]string{
		"relative path":       "sqlite:notes.db",
		"nested relative":     "sqlite:data/notes.db",
		"at sign in filename": "sqlite:notes@2026.db",
		"colon after a slash": "sqlite:data/a:b@c.db",
	} {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			target, err := ParseTarget(raw)
			require.NoError(t, err)
			assert.Equal(t, SchemeSQLite, target.Scheme)
			assert.NotEmpty(t, target.Connection.Host)
		})
	}
}
