package survey

import (
	"errors"
	"fmt"
	"net/url"
	"sort"
	"strconv"
	"strings"

	"github.com/EvilBit-Labs/dbsurveyor/internal/dbadapter"
)

// Target is a parsed connection target: the scheme that selects an adapter, and
// the configuration that reaches the server.
//
// The string it was parsed from is not kept. A connection string carries a
// password in its userinfo, so every moment it exists as a string is a moment it
// can reach a log line -- and the parse is the last point at which it has to.
type Target struct {
	Scheme     string
	Connection dbadapter.ConnectionConfig
}

// Parse errors. They are deliberately terse and never quote the input.
//
// net/url's own parse errors include the URL they failed on, which for a
// connection string means the password. Wrapping one would put a credential into
// an error that a caller prints, which is exactly the path R17 closes.
var (
	ErrEmptyTarget     = errors.New("no connection string was given")
	ErrMalformedTarget = errors.New("the connection string could not be parsed")
	ErrMissingScheme   = errors.New("the connection string names no scheme")
	ErrMissingHost     = errors.New("the connection string names no host")
	ErrUnknownScheme   = errors.New("no adapter speaks that scheme")
)

// The canonical scheme each engine is selected by, and the aliases an operator
// is likely to type. The canonical name is what a Registry is keyed on.
const (
	SchemePostgres  = "postgres"
	SchemeMySQL     = "mysql"
	SchemeSQLite    = "sqlite"
	SchemeMongoDB   = "mongodb"
	SchemeSQLServer = "sqlserver"
	SchemeOracle    = "oracle"
)

// schemeAliases maps every accepted scheme to its canonical form.
//
// The aliases are not a convenience: each is the spelling that engine's own
// tooling uses, so an operator pasting a connection string from a config file or
// a cloud console gets the adapter they meant rather than an error.
var schemeAliases = map[string]string{
	"postgres":      SchemePostgres,
	"postgresql":    SchemePostgres,
	"mysql":         SchemeMySQL,
	"mariadb":       SchemeMySQL,
	"sqlite":        SchemeSQLite,
	"sqlite3":       SchemeSQLite,
	"file":          SchemeSQLite,
	"mongodb":       SchemeMongoDB,
	"mongodb+srv":   SchemeMongoDB,
	"sqlserver":     SchemeSQLServer,
	"mssql":         SchemeSQLServer,
	"oracle":        SchemeOracle,
	"oracle+go-ora": SchemeOracle,
}

// SupportedSchemes lists every accepted scheme, sorted, for an error message an
// operator can act on.
func SupportedSchemes() []string {
	schemes := make([]string, 0, len(schemeAliases))
	for scheme := range schemeAliases {
		schemes = append(schemes, scheme)
	}

	sort.Strings(schemes)

	return schemes
}

// ParseTarget reads a connection string into a target.
//
// The password is moved into a Secret and the string it came from is not
// retained anywhere. Errors describe the shape of the problem and never echo the
// input, because the input is the credential.
func ParseTarget(raw string) (Target, error) {
	if strings.TrimSpace(raw) == "" {
		return Target{}, ErrEmptyTarget
	}

	parsed, err := url.Parse(raw)
	if err != nil {
		// The underlying error is deliberately not wrapped: it quotes the URL,
		// and the URL is the credential.
		return Target{}, ErrMalformedTarget
	}

	if parsed.Scheme == "" {
		return Target{}, ErrMissingScheme
	}

	scheme, known := schemeAliases[strings.ToLower(parsed.Scheme)]
	if !known {
		return Target{}, fmt.Errorf("%w: %q; supported schemes are %s",
			ErrUnknownScheme, parsed.Scheme, strings.Join(SupportedSchemes(), ", "))
	}

	if scheme == SchemeSQLite {
		return sqliteTarget(parsed)
	}

	return serverTarget(scheme, parsed)
}

// serverTarget builds a target for an engine reached over a network.
func serverTarget(scheme string, parsed *url.URL) (Target, error) {
	if parsed.Hostname() == "" {
		return Target{}, ErrMissingHost
	}

	connection := dbadapter.NewConnectionConfig(parsed.Hostname())
	connection.Database = strings.TrimPrefix(parsed.Path, "/")

	if port, err := parsePort(parsed.Port()); err == nil && port != nil {
		connection.Port = port
	}

	if parsed.User != nil {
		connection.Username = parsed.User.Username()

		if password, set := parsed.User.Password(); set {
			connection.Password = dbadapter.NewSecret([]byte(password))
		}
	}

	// A database named in the query string rather than in the path is the form
	// SQL Server's own tooling uses.
	if named := parsed.Query().Get("database"); named != "" && connection.Database == "" {
		connection.Database = named
	}

	return Target{Scheme: scheme, Connection: connection}, nil
}

// sqliteTarget builds a target for a database file.
//
// SQLite has no server, so the host slot carries a path. Both spellings work:
// sqlite:///absolute/path names the path in the URL's path, and sqlite://./rel
// puts the first segment in the host. Neither has a credential to move.
func sqliteTarget(parsed *url.URL) (Target, error) {
	path := parsed.Path
	if parsed.Host != "" {
		path = parsed.Host + path
	}

	if parsed.Opaque != "" {
		// An opaque form carries whatever followed the scheme, unparsed. With no
		// "//" there is no authority for net/url to split, so the userinfo of a
		// mistyped sqlite:user:pass@file.db lands here whole and would go on to
		// be quoted verbatim by the stat error the adapter returns. Refuse it
		// terse rather than let a credential-shaped string reach stderr.
		if hasUserinfo(parsed.Opaque) {
			return Target{}, ErrMalformedTarget
		}

		path = parsed.Opaque
	}

	if path == "" {
		return Target{}, ErrMissingHost
	}

	connection := dbadapter.NewConnectionConfig(path)

	return Target{Scheme: SchemeSQLite, Connection: connection}, nil
}

// hasUserinfo reports whether an opaque target begins with a "user:password@"
// prefix.
//
// Both halves matter. A bare "@" is legal in a filename, so it is the colon
// before it that makes the prefix credential-shaped, and the check stops at the
// first separator so that a path segment after the userinfo cannot supply the
// colon on its behalf.
func hasUserinfo(opaque string) bool {
	at := strings.IndexByte(opaque, '@')
	if at < 0 {
		return false
	}

	prefix := opaque[:at]
	if strings.ContainsAny(prefix, "/\\") {
		return false
	}

	return strings.ContainsRune(prefix, ':')
}

// parsePort reads a port, treating an absent one as "the engine's default
// applies" rather than as an error.
func parsePort(port string) (*uint16, error) {
	if port == "" {
		return nil, nil //nolint:nilnil // an absent port is not a failure; the engine default applies.
	}

	number, err := strconv.ParseUint(port, 10, 16)
	if err != nil {
		return nil, ErrMalformedTarget
	}

	value := uint16(number)

	return &value, nil
}
