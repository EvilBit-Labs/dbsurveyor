package dbschema

import (
	"maps"
	"slices"
)

// minRowsForShapeCheck is the fewest rows a sample needs before its rows can
// disagree with each other.
const minRowsForShapeCheck = 2

func validateTable(table *Table, path string, p *problems) {
	if table.Name == "" {
		p.add(path+".name", "table name must not be empty")
	}

	for i := range table.Columns {
		validateColumn(&table.Columns[i], indexPath(path+".columns", i), p)
	}

	for i := range table.ForeignKeys {
		validateForeignKey(&table.ForeignKeys[i], indexPath(path+".foreign_keys", i), p)
	}

	for i := range table.Constraints {
		if !table.Constraints[i].ConstraintType.Valid() {
			p.addf(indexPath(path+".constraints", i)+".constraint_type",
				"unknown constraint type %q", table.Constraints[i].ConstraintType)
		}
	}
}

func validateColumn(column *Column, path string, p *problems) {
	if column.Name == "" {
		p.add(path+".name", "column name must not be empty")
	}

	// The catalog position is 1-based across every engine. SQLite reports a
	// 0-based cid and its adapter normalizes it; a 0 here means that
	// normalization was skipped.
	if column.OrdinalPosition == 0 {
		p.add(path+".ordinal_position", "ordinal position is 1-based and must be at least 1")
	}

	validateDataType(column.DataType, path+".data_type", p)
}

func validateForeignKey(key *ForeignKey, path string, p *problems) {
	if key.ReferencedTable == "" {
		p.add(path+".referenced_table", "referenced table must not be empty")
	}

	if len(key.Columns) == 0 {
		p.add(path+".columns", "a foreign key must name at least one column")
	}

	// The two lists are positionally paired, so a length mismatch leaves some
	// local column referencing nothing.
	if len(key.Columns) != len(key.ReferencedColumns) {
		p.addf(path, "columns has %d entries but referenced_columns has %d; they are positionally paired",
			len(key.Columns), len(key.ReferencedColumns))
	}

	if key.OnDelete != nil && !key.OnDelete.Valid() {
		p.addf(path+".on_delete", "unknown referential action %q", *key.OnDelete)
	}

	if key.OnUpdate != nil && !key.OnUpdate.Valid() {
		p.addf(path+".on_update", "unknown referential action %q", *key.OnUpdate)
	}
}

func validateDataType(dataType UnifiedDataType, path string, p *problems) {
	if !dataType.Kind.Valid() {
		p.addf(path+".kind", "unknown data type kind %q", dataType.Kind)

		return
	}

	present := map[string]bool{
		"max_length":    dataType.MaxLength != nil,
		"bits":          dataType.Bits != nil,
		"signed":        dataType.Signed != nil,
		"precision":     dataType.Precision != nil,
		"with_timezone": dataType.WithTimezone != nil,
		"element_type":  dataType.ElementType != nil,
		"type_name":     dataType.TypeName != "",
	}

	kind := string(dataType.Kind)

	switch dataType.Kind {
	case TypeString, TypeBinary:
		p.checkPayload(path, kind, present, nil, []string{"max_length"})
	case TypeInteger:
		p.checkPayload(path, kind, present, []string{"bits", "signed"}, nil)
	case TypeFloat:
		p.checkPayload(path, kind, present, nil, []string{"precision"})
	case TypeDateTime, TypeTime:
		p.checkPayload(path, kind, present, []string{"with_timezone"}, nil)
	case TypeArray:
		p.checkPayload(path, kind, present, []string{"element_type"}, nil)

		if dataType.ElementType != nil {
			validateDataType(*dataType.ElementType, path+".element_type", p)
		}
	case TypeCustom:
		p.checkPayload(path, kind, present, []string{"type_name"}, nil)
	case TypeBoolean, TypeDate, TypeJSON, TypeUUID:
		p.checkPayload(path, kind, present, nil, nil)
	}
}

func validateSample(sample *TableSample, path string, p *problems) {
	if sample.TableName == "" {
		p.add(path+".table_name", "sampled table name must not be empty")
	}

	validateSamplingStrategy(sample.SamplingStrategy, path+".sampling_strategy", p)

	if sample.Ordering != nil {
		validateOrdering(*sample.Ordering, path+".ordering", p)
	}

	if sample.Status != nil {
		validateSampleStatus(*sample.Status, path+".status", p)
	}

	validateRowShape(sample.Rows, path+".rows", p)
}

// validateRowShape reports rows whose key sets differ from the first row's.
//
// ColumnNames reads the first row alone, so every consumer of a sample -- the
// quality analyzers, redaction, the report renderer -- silently ignores a column
// that appears only in a later row. Rather than make each of them defensive, a
// sample with ragged rows is defined as malformed and reported here.
func validateRowShape(rows []map[string]any, path string, p *problems) {
	if len(rows) < minRowsForShapeCheck {
		return
	}

	want := slices.Sorted(maps.Keys(rows[0]))

	for i, row := range rows[1:] {
		got := slices.Sorted(maps.Keys(row))
		if !slices.Equal(want, got) {
			p.addf(indexPath(path, i+1), "row has columns %v but the first row has %v; "+
				"every row in a sample must carry the same columns", got, want)
		}
	}
}

func validateSamplingStrategy(strategy SamplingStrategy, path string, p *problems) {
	if !strategy.Kind.Valid() {
		p.addf(path+".kind", "unknown sampling kind %q", strategy.Kind)

		return
	}

	present := map[string]bool{"limit": strategy.Limit != nil}

	switch strategy.Kind {
	case SampleMostRecent, SampleRandom:
		p.checkPayload(path, string(strategy.Kind), present, []string{"limit"}, nil)
	case SampleNone:
		p.checkPayload(path, string(strategy.Kind), present, nil, nil)
	}
}

func validateOrdering(ordering OrderingStrategy, path string, p *problems) {
	if !ordering.Kind.Valid() {
		p.addf(path+".kind", "unknown ordering kind %q", ordering.Kind)

		return
	}

	present := map[string]bool{
		"columns":   len(ordering.Columns) > 0,
		"column":    ordering.Column != nil,
		"direction": ordering.Direction != nil,
	}

	kind := string(ordering.Kind)

	switch ordering.Kind {
	case OrderPrimaryKey:
		p.checkPayload(path, kind, present, []string{"columns"}, nil)
	case OrderTimestamp:
		p.checkPayload(path, kind, present, []string{"column"}, []string{"direction"})
	case OrderAutoIncrement, OrderSystemRowID:
		p.checkPayload(path, kind, present, []string{"column"}, nil)
	case OrderUnordered:
		p.checkPayload(path, kind, present, nil, nil)
	}
}

func validateSampleStatus(status SampleStatus, path string, p *problems) {
	if !status.State.Valid() {
		p.addf(path+".state", "unknown sample state %q", status.State)

		return
	}

	present := map[string]bool{
		"original_limit": status.OriginalLimit != nil,
		"reason":         status.Reason != nil,
	}

	switch status.State {
	case SampleComplete:
		p.checkPayload(path, string(status.State), present, nil, nil)
	case SamplePartialRetry:
		p.checkPayload(path, string(status.State), present, []string{"original_limit"}, nil)
	case SampleSkipped:
		p.checkPayload(path, string(status.State), present, []string{"reason"}, nil)
	}
}

// validateRollUp reports disagreement between the schema-wide index and
// constraint lists and the per-table lists they duplicate.
//
// The duplication exists so a report can iterate the roll-up once instead of
// walking every table, which only holds if the two agree. A document whose
// roll-up is stale would render a report missing objects that are plainly
// present in its own tables.
func validateRollUp(s *Schema, p *problems) {
	wantIndexes := make([]string, 0, len(s.Indexes))
	wantConstraints := make([]string, 0, len(s.Constraints))

	for i := range s.Tables {
		for j := range s.Tables[i].Indexes {
			wantIndexes = append(wantIndexes, objectKey(s.Tables[i].Indexes[j].TableName, s.Tables[i].Indexes[j].Name))
		}

		for j := range s.Tables[i].Constraints {
			wantConstraints = append(wantConstraints,
				objectKey(s.Tables[i].Constraints[j].TableName, s.Tables[i].Constraints[j].Name))
		}
	}

	gotIndexes := make([]string, 0, len(s.Indexes))
	for i := range s.Indexes {
		gotIndexes = append(gotIndexes, objectKey(s.Indexes[i].TableName, s.Indexes[i].Name))
	}

	gotConstraints := make([]string, 0, len(s.Constraints))
	for i := range s.Constraints {
		gotConstraints = append(gotConstraints, objectKey(s.Constraints[i].TableName, s.Constraints[i].Name))
	}

	if !sameMultiset(wantIndexes, gotIndexes) {
		p.addf("indexes", "roll-up holds %d indexes but the tables hold %d; "+
			"call AggregateIndexesAndConstraints after the tables are complete", len(gotIndexes), len(wantIndexes))
	}

	if !sameMultiset(wantConstraints, gotConstraints) {
		p.addf("constraints", "roll-up holds %d constraints but the tables hold %d; "+
			"call AggregateIndexesAndConstraints after the tables are complete", len(gotConstraints), len(wantConstraints))
	}
}

// objectKey identifies a catalog object by its table and name. A name alone is
// not unique across tables on any supported engine.
func objectKey(tableName, name string) string {
	return tableName + "\x00" + name
}

func sameMultiset(want, got []string) bool {
	if len(want) != len(got) {
		return false
	}

	slices.Sort(want)
	slices.Sort(got)

	return slices.Equal(want, got)
}
