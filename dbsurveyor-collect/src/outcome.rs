//! Exit outcome classification for collection runs.

use dbsurveyor_core::{
    DatabaseSchema,
    models::{CollectionStatus, SampleStatus},
};

/// Final outcome of a collection run, mapped to process exit codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CollectionOutcome {
    Success,
    TotalFailure { error: String },
    PartialWithoutSamples,
    PartialWithData,
    PartialWithValidationWarnings,
    Canceled { reason: String },
}

impl CollectionOutcome {
    /// Returns the process exit code for this outcome.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Self::Success => 0,
            Self::TotalFailure { .. } => 1,
            Self::PartialWithoutSamples => 2,
            Self::PartialWithData => 3,
            Self::PartialWithValidationWarnings => 4,
            Self::Canceled { .. } => 5,
        }
    }

    /// Aggregates per-database schemas into a single run outcome.
    pub(crate) fn from_results(databases: &[DatabaseSchema]) -> Self {
        if databases.is_empty() {
            return Self::TotalFailure {
                error: "No schemas were produced".to_string(),
            };
        }

        let has_without_samples = databases.iter().any(database_has_no_samples);
        if has_without_samples {
            return Self::PartialWithoutSamples;
        }

        let has_partial_data = databases.iter().any(database_has_partial_data);
        if has_partial_data {
            return Self::PartialWithData;
        }

        let has_validation_warnings = databases
            .iter()
            .any(|database| !database.collection_metadata.warnings.is_empty());
        if has_validation_warnings {
            return Self::PartialWithValidationWarnings;
        }

        Self::Success
    }
}

fn database_has_no_samples(database: &DatabaseSchema) -> bool {
    match database.database_info.collection_status {
        CollectionStatus::Success => database.samples.as_ref().is_some_and(|samples| {
            !samples.is_empty()
                && samples.iter().all(|sample| {
                    matches!(sample.sample_status, Some(SampleStatus::Skipped { .. }))
                })
        }),
        CollectionStatus::Failed { .. } | CollectionStatus::Skipped { .. } => false,
    }
}

fn database_has_partial_data(database: &DatabaseSchema) -> bool {
    if matches!(
        database.database_info.collection_status,
        CollectionStatus::Failed { .. } | CollectionStatus::Skipped { .. }
    ) {
        return true;
    }

    database.samples.as_ref().is_some_and(|samples| {
        samples.iter().any(|sample| {
            matches!(
                sample.sample_status,
                Some(SampleStatus::PartialRetry { .. } | SampleStatus::Skipped { .. })
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbsurveyor_core::{
        DatabaseInfo, SamplingStrategy, TableSample,
        models::{AccessLevel, CollectionMetadata},
    };

    fn database_schema(name: &str) -> DatabaseSchema {
        DatabaseSchema::new(DatabaseInfo::new(name.to_string()))
    }

    fn sample(status: SampleStatus) -> TableSample {
        TableSample {
            table_name: "users".to_string(),
            schema_name: Some("public".to_string()),
            rows: Vec::new(),
            sample_size: 0,
            total_rows: None,
            sampling_strategy: SamplingStrategy::None,
            collected_at: chrono::Utc::now(),
            warnings: Vec::new(),
            sample_status: Some(status),
        }
    }

    #[test]
    fn test_outcome_success() {
        let schema = database_schema("db1");
        assert_eq!(
            CollectionOutcome::from_results(&[schema]),
            CollectionOutcome::Success
        );
    }

    #[test]
    fn test_outcome_total_failure_for_empty_results() {
        assert!(matches!(
            CollectionOutcome::from_results(&[]),
            CollectionOutcome::TotalFailure { .. }
        ));
    }

    #[test]
    fn test_outcome_partial_without_samples_takes_precedence() {
        let mut no_samples = database_schema("db1");
        no_samples.samples = Some(vec![sample(SampleStatus::Skipped {
            reason: "sampling disabled".to_string(),
        })]);

        let mut with_warning = database_schema("db2");
        with_warning.collection_metadata = CollectionMetadata {
            collected_at: chrono::Utc::now(),
            collection_duration_ms: 0,
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            warnings: vec!["warning".to_string()],
        };

        assert_eq!(
            CollectionOutcome::from_results(&[no_samples, with_warning]),
            CollectionOutcome::PartialWithoutSamples
        );
    }

    #[test]
    fn test_outcome_partial_with_data_for_failed_database() {
        let mut failed = database_schema("db1");
        failed.database_info.access_level = AccessLevel::None;
        failed.database_info.collection_status = CollectionStatus::Failed {
            error: "sanitized failure".to_string(),
        };

        let success = database_schema("db2");

        assert_eq!(
            CollectionOutcome::from_results(&[success, failed]),
            CollectionOutcome::PartialWithData
        );
    }

    #[test]
    fn test_outcome_partial_with_data_for_skipped_database() {
        let mut skipped = database_schema("db1");
        skipped.database_info.access_level = AccessLevel::Limited;
        skipped.database_info.collection_status = CollectionStatus::Skipped {
            reason: "privilege limited".to_string(),
        };

        assert_eq!(
            CollectionOutcome::from_results(&[database_schema("db2"), skipped]),
            CollectionOutcome::PartialWithData
        );
    }

    #[test]
    fn test_outcome_partial_with_validation_warnings() {
        let mut warning_schema = database_schema("db1");
        warning_schema
            .collection_metadata
            .warnings
            .push("warning".to_string());

        assert_eq!(
            CollectionOutcome::from_results(&[warning_schema]),
            CollectionOutcome::PartialWithValidationWarnings
        );
    }

    #[test]
    fn test_outcome_success_when_sampling_disabled_and_no_samples_present() {
        let schema = database_schema("db1");
        assert_eq!(
            CollectionOutcome::from_results(&[schema]),
            CollectionOutcome::Success
        );
    }

    #[test]
    fn test_outcome_partial_without_samples_beats_validation_warnings() {
        let mut no_samples = database_schema("db1");
        no_samples.samples = Some(vec![sample(SampleStatus::Skipped {
            reason: "sample skipped".to_string(),
        })]);

        let mut warning_schema = database_schema("db2");
        warning_schema
            .collection_metadata
            .warnings
            .push("validation warning".to_string());

        assert_eq!(
            CollectionOutcome::from_results(&[warning_schema, no_samples]),
            CollectionOutcome::PartialWithoutSamples
        );
    }

    #[test]
    fn test_exit_code_mapping() {
        assert_eq!(CollectionOutcome::Success.exit_code(), 0);
        assert_eq!(
            CollectionOutcome::TotalFailure {
                error: "boom".to_string()
            }
            .exit_code(),
            1
        );
        assert_eq!(CollectionOutcome::PartialWithoutSamples.exit_code(), 2);
        assert_eq!(CollectionOutcome::PartialWithData.exit_code(), 3);
        assert_eq!(
            CollectionOutcome::PartialWithValidationWarnings.exit_code(),
            4
        );
        assert_eq!(
            CollectionOutcome::Canceled {
                reason: "user canceled".to_string()
            }
            .exit_code(),
            5
        );
    }
}
