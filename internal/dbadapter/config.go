package dbadapter

import (
	"errors"
	"fmt"
	"regexp"
	"time"
)

// Sample size bounds.
//
// The maximum is a memory bound, not a policy: every sampled row is held in
// memory as a map before it reaches the artifact writer, so an unbounded sample
// size turns a wide table into an out-of-memory crash.
const (
	MinSampleSize     uint32 = 1
	MaxSampleSize     uint32 = 10_000
	DefaultSampleSize uint32 = 100
)

// Timeout defaults. Both are deliberately short. A survey runs against a
// database an operator does not own, and a query that hangs is worse than a
// query that fails: the operator is left unsure whether the tool is working.
const (
	DefaultConnectTimeout = 10 * time.Second
	DefaultQueryTimeout   = 30 * time.Second
)

// Pool defaults. Small on purpose -- in multi-database mode there is one pool
// per database, and a generous per-pool maximum exhausts the server's
// connection limit long before it helps throughput.
const (
	DefaultMaxConnections uint32 = 5
	DefaultMaxIdleTime           = 5 * time.Minute
)

// MaxQueryTimeout bounds a configured query timeout. A timeout measured in
// hours is indistinguishable from no timeout at all.
const MaxQueryTimeout = 30 * time.Minute

// Validation errors. They are sentinels so a caller can tell a configuration
// mistake from a connection failure.
var (
	ErrInvalidSampleSize = errors.New("sample size is out of range")
	ErrInvalidTimeout    = errors.New("timeout is out of range")
	ErrInvalidPoolSize   = errors.New("connection pool size is invalid")
	ErrMissingHost       = errors.New("connection host is empty")
	ErrInvalidPattern    = errors.New("sensitive-value pattern does not compile")
	ErrEmptyTableName    = errors.New("table name is empty")
	ErrNotReadOnly       = errors.New("read-only access is required")
)

// TableRef names one table for sampling.
//
// Schema is empty for engines that have no schema concept -- SQLite, and
// MongoDB collections -- rather than carrying a placeholder that each adapter
// would have to recognize.
type TableRef struct {
	Schema string
	Table  string
}

// String renders the reference as an engine would name it: "schema.table" when
// a schema is present, the bare table name otherwise.
//
// This is for messages and log lines. It is not an identifier for a query: the
// parts are not quoted, and quoting rules differ per engine.
func (r TableRef) String() string {
	if r.Schema == "" {
		return r.Table
	}

	return r.Schema + "." + r.Table
}

// Validate reports whether the reference names a table.
func (r TableRef) Validate() error {
	if r.Table == "" {
		return ErrEmptyTableName
	}

	return nil
}

// ConnectionConfig describes how to reach a database.
//
// Password is a Secret, so a ConnectionConfig can be logged, formatted, and
// marshalled without leaking it. That is the point of the type: the safe thing
// is what happens when nobody thinks about it.
type ConnectionConfig struct {
	// Host is a hostname or address. It is not the database name; see Database.
	Host string
	// Port is nil when the engine's default applies.
	Port *uint16
	// Database is the database or catalog name, empty when the engine's
	// default applies.
	Database string
	Username string
	Password Secret

	ConnectTimeout time.Duration
	QueryTimeout   time.Duration

	MaxConnections uint32
	// MinIdleConnections is zero by default. In multi-database mode a pool is
	// built per database and closed when that database is done, so holding idle
	// connections open costs the server and buys nothing.
	MinIdleConnections uint32
	MaxIdleTime        time.Duration

	// ReadOnly requests a read-only session where the engine supports one. It
	// is a second line of defense, not the first: every query this tool issues
	// is a read regardless.
	ReadOnly bool
}

// NewConnectionConfig returns a configuration for host with the defaults
// applied.
func NewConnectionConfig(host string) ConnectionConfig {
	return ConnectionConfig{
		Host:           host,
		ConnectTimeout: DefaultConnectTimeout,
		QueryTimeout:   DefaultQueryTimeout,
		MaxConnections: DefaultMaxConnections,
		MaxIdleTime:    DefaultMaxIdleTime,
		ReadOnly:       true,
	}
}

// Validate reports every way the configuration is unusable.
func (c ConnectionConfig) Validate() error {
	var problems []error

	if c.Host == "" {
		problems = append(problems, ErrMissingHost)
	}

	if c.ConnectTimeout <= 0 {
		problems = append(problems, fmt.Errorf("%w: connect timeout is %s", ErrInvalidTimeout, c.ConnectTimeout))
	}

	if c.QueryTimeout <= 0 || c.QueryTimeout > MaxQueryTimeout {
		problems = append(problems, fmt.Errorf("%w: query timeout is %s, want (0, %s]",
			ErrInvalidTimeout, c.QueryTimeout, MaxQueryTimeout))
	}

	if c.MaxConnections == 0 {
		problems = append(problems, fmt.Errorf("%w: max connections is 0", ErrInvalidPoolSize))
	}

	if c.MinIdleConnections > c.MaxConnections {
		problems = append(problems, fmt.Errorf("%w: min idle %d exceeds max %d",
			ErrInvalidPoolSize, c.MinIdleConnections, c.MaxConnections))
	}

	if !c.ReadOnly {
		problems = append(problems, ErrNotReadOnly)
	}

	return errors.Join(problems...)
}

// SensitivePattern is a regular expression that marks a sampled value as
// probably sensitive, with a description an operator can act on.
type SensitivePattern struct {
	Pattern     string `json:"pattern"`
	Description string `json:"description"`
}

// SamplingConfig governs how rows are drawn from a table.
//
// It is plain data. The compiled form of SensitivePatterns lives in a separate
// Matcher rather than in a cache field here, which is what keeps a
// deserialized config from behaving differently to a constructed one: there is
// no cache to arrive empty.
type SamplingConfig struct {
	// SampleSize is the requested row limit, within [MinSampleSize,
	// MaxSampleSize].
	SampleSize uint32 `json:"sample_size"`
	// Throttle delays between sampling queries, to keep a survey from
	// monopolizing a production database.
	Throttle time.Duration `json:"throttle"`
	// QueryTimeout bounds one sampling query.
	QueryTimeout time.Duration `json:"query_timeout"`
	// WarnSensitive reports values matching SensitivePatterns as warnings on
	// the sample.
	WarnSensitive bool `json:"warn_sensitive"`
	// TimestampColumns names columns preferred for ordering when the table has
	// no usable key.
	TimestampColumns []string `json:"timestamp_columns,omitempty"`
	// SensitivePatterns are matched against sampled values when WarnSensitive
	// is set.
	SensitivePatterns []SensitivePattern `json:"sensitive_patterns,omitempty"`
}

// NewSamplingConfig returns a configuration for the requested sample size,
// clamped into range, with the defaults applied.
//
// Clamping rather than rejecting is right here: a caller who asks for more rows
// than the maximum wants as many as possible, and the maximum is a memory bound
// rather than a statement about their intent.
func NewSamplingConfig(sampleSize uint32) SamplingConfig {
	return SamplingConfig{
		SampleSize:   clamp(sampleSize, MinSampleSize, MaxSampleSize),
		QueryTimeout: DefaultQueryTimeout,
	}
}

// DefaultSamplingConfig returns the configuration used when no sample size was
// requested.
func DefaultSamplingConfig() SamplingConfig {
	return NewSamplingConfig(DefaultSampleSize)
}

// Validate reports every way the configuration is unusable.
//
// It must reject values the constructor would never produce. A struct literal
// and a JSON document both bypass NewSamplingConfig, so the clamping there is
// not a guarantee about any SamplingConfig that reaches an adapter -- only
// about the ones the constructor made.
func (c SamplingConfig) Validate() error {
	var problems []error

	if c.SampleSize < MinSampleSize || c.SampleSize > MaxSampleSize {
		problems = append(problems, fmt.Errorf("%w: %d, want [%d, %d]",
			ErrInvalidSampleSize, c.SampleSize, MinSampleSize, MaxSampleSize))
	}

	if c.Throttle < 0 {
		problems = append(problems, fmt.Errorf("%w: throttle is %s", ErrInvalidTimeout, c.Throttle))
	}

	if c.QueryTimeout <= 0 || c.QueryTimeout > MaxQueryTimeout {
		problems = append(problems, fmt.Errorf("%w: query timeout is %s, want (0, %s]",
			ErrInvalidTimeout, c.QueryTimeout, MaxQueryTimeout))
	}

	if _, err := c.Matcher(); err != nil {
		problems = append(problems, err)
	}

	return errors.Join(problems...)
}

// SensitiveMatcher is the compiled form of a SamplingConfig's patterns.
//
// It is a separate value, built on demand, because a compiled cache living
// inside the config is the shape that produced a real bug in the previous
// implementation: the cache was skipped during deserialization, so a config
// read from a file silently matched nothing.
type SensitiveMatcher struct {
	rules []compiledPattern
}

// compiledPattern pairs a compiled expression with the description reported
// when it fires.
type compiledPattern struct {
	expression  *regexp.Regexp
	description string
}

// Matcher compiles the configuration's sensitive-value patterns.
func (c SamplingConfig) Matcher() (SensitiveMatcher, error) {
	rules := make([]compiledPattern, 0, len(c.SensitivePatterns))

	for _, pattern := range c.SensitivePatterns {
		expression, err := regexp.Compile(pattern.Pattern)
		if err != nil {
			return SensitiveMatcher{}, fmt.Errorf("%w: %q: %w", ErrInvalidPattern, pattern.Pattern, err)
		}

		rules = append(rules, compiledPattern{expression: expression, description: pattern.description()})
	}

	return SensitiveMatcher{rules: rules}, nil
}

// description falls back to the pattern itself so a warning is never empty.
func (p SensitivePattern) description() string {
	if p.Description == "" {
		return p.Pattern
	}

	return p.Description
}

// Match reports the description of the first pattern matching value, and
// whether any did.
func (m SensitiveMatcher) Match(value string) (string, bool) {
	for _, rule := range m.rules {
		if rule.expression.MatchString(value) {
			return rule.description, true
		}
	}

	return "", false
}

// Len reports how many patterns the matcher holds.
func (m SensitiveMatcher) Len() int {
	return len(m.rules)
}

// CollectionConfig is everything one adapter needs for one survey.
//
// It stops at the adapter boundary. Output format, compression, and encryption
// are not here: those belong to internal/artifact (R8), and an adapter that
// could see them would be an adapter that could be made to care about them.
type CollectionConfig struct {
	Connection ConnectionConfig
	Sampling   SamplingConfig

	// Sample enables data sampling. Schema collection happens either way; this
	// governs whether any row values are read at all.
	Sample bool

	IncludeViews      bool
	IncludeRoutines   bool
	IncludeTriggers   bool
	IncludeIndexes    bool
	IncludeConstraint bool
	IncludeTypes      bool

	// IncludeSystemDatabases includes the engine's own catalogs in a
	// multi-database survey. Off by default: they are the same on every server
	// and drown the interesting output.
	IncludeSystemDatabases bool
	// ExcludeDatabases names databases to skip by exact name.
	ExcludeDatabases []string

	// MaxConcurrentQueries bounds in-flight queries against one database.
	MaxConcurrentQueries uint32
}

// NewCollectionConfig returns a configuration for host with every metadata kind
// included and sampling off.
//
// Sampling is off by default because it is the only part of a survey that reads
// user data. An operator who wants row values asks for them.
func NewCollectionConfig(host string) CollectionConfig {
	return CollectionConfig{
		Connection:           NewConnectionConfig(host),
		Sampling:             DefaultSamplingConfig(),
		IncludeViews:         true,
		IncludeRoutines:      true,
		IncludeTriggers:      true,
		IncludeIndexes:       true,
		IncludeConstraint:    true,
		IncludeTypes:         true,
		MaxConcurrentQueries: DefaultMaxConnections,
	}
}

// Validate reports every way the configuration is unusable, including the
// problems of the configurations it holds.
func (c CollectionConfig) Validate() error {
	problems := []error{c.Connection.Validate()}

	if c.Sample {
		problems = append(problems, c.Sampling.Validate())
	}

	if c.MaxConcurrentQueries == 0 {
		problems = append(problems, fmt.Errorf("%w: max concurrent queries is 0", ErrInvalidPoolSize))
	}

	return errors.Join(problems...)
}

// clamp confines v to [low, high].
func clamp(v, low, high uint32) uint32 {
	return min(max(v, low), high)
}
