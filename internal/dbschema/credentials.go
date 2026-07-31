package dbschema

import (
	"encoding/json"
	"fmt"
	"regexp"
	"slices"
	"strconv"
)

// CredentialFinding locates a string in a marshalled document that matches a
// credential pattern.
//
// It carries the path and the name of the rule that fired, never the offending
// value. Reporting the value would copy a credential into an error string, a log
// line, and eventually an operator's terminal or CI transcript -- the exact
// outcome the scan exists to prevent.
type CredentialFinding struct {
	// Path locates the value in the document, in the form
	// "samples[0].rows[1].note". The root document itself has the path ".".
	Path string
	// Rule names the pattern that matched.
	Rule string
}

// String renders a finding for an error message. It reports where and which
// rule, never what.
func (f CredentialFinding) String() string {
	return fmt.Sprintf("%s matches credential pattern %q", f.Path, f.Rule)
}

// credentialRule is one compiled credential pattern.
type credentialRule struct {
	name    string
	pattern *regexp.Regexp
}

// credentialRules are compiled once at package initialization and are never
// serialized, so there is no cache that can arrive empty after a document is
// decoded. Every scan uses the same compiled set.
//
// The rules deliberately favor false positives over false negatives. A false
// positive rejects a document loudly and an operator can see why; a false
// negative writes a credential to disk silently. A CHECK clause reading
// "password = 'x'" will trip the assignment rule, and that is the intended
// trade.
var credentialRules = []credentialRule{
	{
		// A URL carrying userinfo with a password: scheme://user:pass@host.
		// The scheme is not restricted to database schemes, because an https URL
		// with embedded credentials is just as much a leak.
		name:    "connection-url-credentials",
		pattern: regexp.MustCompile(`(?i)\b[a-z][a-z0-9+.\-]*://[^\s:/?#@]+:[^\s/?#@]*@`),
	},
	{
		// A keyword assignment, covering both DSN form ("Password=secret;") and
		// environment form ("API_KEY=abc123").
		name: "credential-assignment",
		pattern: regexp.MustCompile(
			`(?i)\b(pass(word|wd)?|pwd|secret|token|api[_\-]?key|access[_\-]?key|private[_\-]?key)\s*=\s*[^\s;&"']`,
		),
	},
	{
		// An inlined PEM private key.
		name:    "private-key-block",
		pattern: regexp.MustCompile(`-{5}BEGIN [A-Z ]*PRIVATE KEY-{5}`),
	},
}

// MatchCredentialRule reports the name of the first credential rule matching
// value, or the empty string when none does.
func MatchCredentialRule(value string) string {
	for _, rule := range credentialRules {
		if rule.pattern.MatchString(value) {
			return rule.name
		}
	}

	return ""
}

// ScanForCredentials marshals document to JSON and reports every string value
// that matches a credential pattern, with the path to the offending node.
//
// Scanning the marshalled form rather than the Go value is deliberate: what
// reaches disk is the JSON, so the JSON is what must be clean. A field the Go
// type declares but omits from output cannot leak, and a field added to a type
// later is scanned without this function being updated.
//
// Only values are scanned, never keys. A column named "password" is schema
// metadata that an operator needs to see; the value stored in it is what must
// never appear.
func ScanForCredentials(document any) ([]CredentialFinding, error) {
	data, err := json.Marshal(document)
	if err != nil {
		return nil, fmt.Errorf("marshal document for credential scan: %w", err)
	}

	return ScanJSONForCredentials(data)
}

// ScanJSONForCredentials reports every string in the JSON document that matches
// a credential pattern. It is the entry point for load paths, which hold the
// document as bytes before they hold it as a type.
func ScanJSONForCredentials(data []byte) ([]CredentialFinding, error) {
	var document any
	if err := json.Unmarshal(data, &document); err != nil {
		return nil, fmt.Errorf("decode document for credential scan: %w", err)
	}

	var findings []CredentialFinding
	scanNode(document, ".", &findings)

	return findings, nil
}

// scanNode walks a decoded JSON value depth-first, appending a finding for every
// string that matches a rule. Object keys are visited in sorted order so the
// finding list does not depend on Go's map iteration order.
func scanNode(node any, path string, findings *[]CredentialFinding) {
	switch value := node.(type) {
	case string:
		if rule := MatchCredentialRule(value); rule != "" {
			*findings = append(*findings, CredentialFinding{Path: path, Rule: rule})
		}
	case map[string]any:
		keys := make([]string, 0, len(value))
		for key := range value {
			keys = append(keys, key)
		}

		slices.Sort(keys)

		for _, key := range keys {
			scanNode(value[key], childPath(path, key), findings)
		}
	case []any:
		for i, item := range value {
			scanNode(item, path+"["+strconv.Itoa(i)+"]", findings)
		}
	}
}

// childPath joins a parent path and an object key. The root path is "." and its
// children carry no leading separator, so a top-level field reads as "tables"
// rather than ".tables".
func childPath(parent, key string) string {
	if parent == "." {
		return key
	}

	return parent + "." + key
}
