package dbschema

import (
	"errors"
	"fmt"
	"maps"
	"slices"
	"strconv"
)

// ValidationError is one reason a document is invalid, located by its path
// within the document.
type ValidationError struct {
	// Path locates the offending node, in the form "tables[0].columns[2]".
	Path string
	// Detail states what is wrong with it.
	Detail string
}

// Error renders the problem with its location.
func (e *ValidationError) Error() string {
	return e.Path + ": " + e.Detail
}

// CredentialError reports that a document carries a value matching a credential
// pattern. It is the failure a load path must never suppress.
//
// Like CredentialFinding, it names the location and the rule and never the
// value.
type CredentialError struct {
	Finding CredentialFinding
}

// Error renders the finding without reproducing the offending value.
func (e *CredentialError) Error() string {
	return e.Finding.String() + "; a document must never carry a credential"
}

// problems accumulates validation failures so a single pass can report every
// problem in a document rather than only the first.
type problems []error

// add records a problem at path.
func (p *problems) add(path, detail string) {
	*p = append(*p, &ValidationError{Path: path, Detail: detail})
}

// addf records a problem at path with a formatted detail.
func (p *problems) addf(path, format string, args ...any) {
	p.add(path, fmt.Sprintf(format, args...))
}

// checkPayload verifies the payload fields of a discriminated union against the
// fields its discriminator owns.
//
// present maps every payload field of the union to whether it is set on this
// value. A field listed in required must be set, a field listed in optional may
// be, and a field in neither belongs to a different variant and must be absent.
// This is the check that stands in for the sum type Go does not have: the
// constructors establish the invariant, and this enforces it on documents that
// arrived from disk without passing through them.
func (p *problems) checkPayload(path, discriminator string, present map[string]bool, required, optional []string) {
	for _, field := range slices.Sorted(maps.Keys(present)) {
		switch {
		case slices.Contains(required, field):
			if !present[field] {
				p.addf(path, "%q requires %s", discriminator, field)
			}
		case slices.Contains(optional, field):
		case present[field]:
			p.addf(path, "%q must not carry %s", discriminator, field)
		}
	}
}

// indexPath appends an array index to a path.
func indexPath(base string, index int) string {
	return base + "[" + strconv.Itoa(index) + "]"
}

// Validate reports every way in which the document departs from the format
// specification, joined into a single error.
//
// The checks cover document identity, required non-empty fields, the payload
// invariants of every discriminated union, agreement between the schema-wide
// index and constraint roll-ups and the per-table lists, consistency of the key
// sets across a sample's rows, and -- decisively -- the recursive credential
// scan. A document carrying a credential fails here, which is why every load
// path calls Validate rather than deciding for itself whether to scan.
func (s *Schema) Validate() error {
	var p problems

	if s.Format != Format {
		p.addf(".", "format is %q, want %q", s.Format, Format)
	}

	if s.FormatVersion != FormatVersion {
		p.addf(".", "format_version is %q, want %q", s.FormatVersion, FormatVersion)
	}

	validateDatabaseInfo(s.DatabaseInfo, "database_info", &p)
	validateCollectionMetadata(s.CollectionMetadata, "collection_metadata", &p)

	for i := range s.Tables {
		validateTable(&s.Tables[i], indexPath("tables", i), &p)
	}

	for i := range s.Samples {
		validateSample(&s.Samples[i], indexPath("samples", i), &p)
	}

	validateRollUp(s, &p)
	appendCredentialProblems(s, &p)

	return errors.Join(p...)
}

// Validate reports every way in which the multi-database document departs from
// the format specification, including the problems of each database it carries.
func (v *ServerSchema) Validate() error {
	var p problems

	if v.Format != ServerFormat {
		p.addf(".", "format is %q, want %q", v.Format, ServerFormat)
	}

	if v.FormatVersion != FormatVersion {
		p.addf(".", "format_version is %q, want %q", v.FormatVersion, FormatVersion)
	}

	if !v.ServerInfo.ServerType.Valid() {
		p.addf("server_info.server_type", "unknown database type %q", v.ServerInfo.ServerType)
	}

	validateCollectionMode(v.ServerInfo.CollectionMode, "server_info.collection_mode", &p)
	validateCollectionMetadata(v.CollectionMetadata, "collection_metadata", &p)

	for i := range v.Databases {
		if err := v.Databases[i].Validate(); err != nil {
			p = append(p, fmt.Errorf("%s: %w", indexPath("databases", i), err))
		}
	}

	appendCredentialProblems(v, &p)

	return errors.Join(p...)
}

// appendCredentialProblems runs the recursive credential scan over the
// marshalled document and records one problem per finding.
func appendCredentialProblems(document any, p *problems) {
	findings, err := ScanForCredentials(document)
	if err != nil {
		*p = append(*p, err)

		return
	}

	for _, finding := range findings {
		*p = append(*p, &CredentialError{Finding: finding})
	}
}

func validateDatabaseInfo(info DatabaseInfo, path string, p *problems) {
	if info.Name == "" {
		p.add(path+".name", "database name must not be empty")
	}

	if !info.Type.Valid() {
		p.addf(path+".type", "unknown database type %q", info.Type)
	}

	if !info.AccessLevel.Valid() {
		p.addf(path+".access_level", "unknown access level %q", info.AccessLevel)
	}

	validateCollectionStatus(info.CollectionStatus, path+".collection_status", p)
}

func validateCollectionStatus(status CollectionStatus, path string, p *problems) {
	if !status.State.Valid() {
		p.addf(path+".state", "unknown collection state %q", status.State)

		return
	}

	present := map[string]bool{
		"error":  status.Error != nil,
		"reason": status.Reason != nil,
	}

	switch status.State {
	case CollectionSuccess:
		p.checkPayload(path, string(status.State), present, nil, nil)
	case CollectionFailed:
		p.checkPayload(path, string(status.State), present, []string{"error"}, nil)
	case CollectionSkipped:
		p.checkPayload(path, string(status.State), present, []string{"reason"}, nil)
	}
}

func validateCollectionMode(mode CollectionMode, path string, p *problems) {
	if !mode.Kind.Valid() {
		p.addf(path+".kind", "unknown collection mode kind %q", mode.Kind)

		return
	}

	present := map[string]bool{
		"discovered": mode.Discovered != nil,
		"collected":  mode.Collected != nil,
		"failed":     mode.Failed != nil,
	}

	switch mode.Kind {
	case ModeSingleDatabase:
		p.checkPayload(path, string(mode.Kind), present, nil, nil)
	case ModeMultiDatabase:
		p.checkPayload(path, string(mode.Kind), present, []string{"discovered", "collected", "failed"}, nil)
	}
}

func validateCollectionMetadata(metadata CollectionMetadata, path string, p *problems) {
	if metadata.CollectorVersion == "" {
		p.add(path+".collector_version", "collector version must not be empty")
	}

	if metadata.CollectedAt.IsZero() {
		p.add(path+".collected_at", "collection time must be set")
	}
}
