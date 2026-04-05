//! Sampling orchestration for collector-driven retry and warning policy.

use dbsurveyor_core::{
    DatabaseAdapter, SamplingConfig, SamplingStrategy, Table, TableSample, adapters::TableRef,
    models::SampleStatus,
};

/// Aggregated result of a sampling run.
pub(crate) struct SamplingRun {
    pub(crate) samples: Vec<TableSample>,
    pub(crate) warnings: Vec<String>,
}

/// Coordinates per-table sampling policy above database adapters.
pub(crate) struct SamplingOrchestrator<'a> {
    adapter: &'a dyn DatabaseAdapter,
    config: &'a SamplingConfig,
}

impl<'a> SamplingOrchestrator<'a> {
    /// Creates a new sampling orchestrator.
    pub(crate) fn new(adapter: &'a dyn DatabaseAdapter, config: &'a SamplingConfig) -> Self {
        Self { adapter, config }
    }

    /// Samples all provided tables and applies collector retry policy.
    pub(crate) async fn run(&self, tables: &[Table]) -> SamplingRun {
        let mut samples = Vec::with_capacity(tables.len());
        let mut warnings = Vec::new();

        for table in tables {
            let table_ref = TableRef {
                schema_name: table.schema.as_deref(),
                table_name: &table.name,
            };

            let sample = self.sample_single_table(table_ref).await;
            warnings.extend(sample.warnings.iter().cloned());
            samples.push(sample);
        }

        SamplingRun { samples, warnings }
    }

    async fn sample_single_table(&self, table_ref: TableRef<'_>) -> TableSample {
        match self
            .adapter
            .sample_table(table_ref.clone(), self.config)
            .await
        {
            Ok(sample) => self.mark_complete_if_unset(table_ref, sample),
            Err(first_error) => {
                let retry_size = (self.config.sample_size / 2).max(1);
                let retry_config = self.config.clone().with_sample_size(retry_size);

                match self
                    .adapter
                    .sample_table(table_ref.clone(), &retry_config)
                    .await
                {
                    Ok(sample) => {
                        let retry_warning = format!(
                            "Sampling table '{}' retried with reduced limit {} after initial failure: {}",
                            table_ref, retry_size, first_error
                        );
                        self.finalize_sample(
                            table_ref,
                            sample,
                            SampleStatus::PartialRetry {
                                original_limit: self.config.sample_size,
                            },
                        )
                        .with_warning(retry_warning)
                    }
                    Err(retry_error) => {
                        let reason = format!(
                            "Sampling failed for '{}' after retry with limit {}: initial error: {}; retry error: {}",
                            table_ref, retry_size, first_error, retry_error
                        );
                        skipped_sample(table_ref, reason)
                    }
                }
            }
        }
    }

    fn mark_complete_if_unset(
        &self,
        table_ref: TableRef<'_>,
        mut sample: TableSample,
    ) -> TableSample {
        if sample.sample_status.is_none() {
            sample.sample_status = Some(SampleStatus::Complete);
        }

        self.add_random_fallback_warning(table_ref, sample)
    }

    fn finalize_sample(
        &self,
        table_ref: TableRef<'_>,
        mut sample: TableSample,
        status: SampleStatus,
    ) -> TableSample {
        sample.sample_status = Some(status);

        self.add_random_fallback_warning(table_ref, sample)
    }

    fn add_random_fallback_warning(
        &self,
        table_ref: TableRef<'_>,
        mut sample: TableSample,
    ) -> TableSample {
        if matches!(sample.sampling_strategy, SamplingStrategy::Random { .. }) {
            let warning = format!(
                "Table '{}' has no reliable ordering; random sampling fallback was used",
                table_ref
            );
            if !sample.warnings.iter().any(|existing| existing == &warning) {
                sample.warnings.push(warning);
            }
        }

        sample
    }
}

trait TableSampleWarningExt {
    fn with_warning(self, warning: String) -> Self;
}

impl TableSampleWarningExt for TableSample {
    fn with_warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }
}

fn skipped_sample(table_ref: TableRef<'_>, reason: String) -> TableSample {
    TableSample {
        table_name: table_ref.table_name.to_string(),
        schema_name: table_ref.schema_name.map(str::to_string),
        rows: Vec::new(),
        sample_size: 0,
        total_rows: None,
        sampling_strategy: SamplingStrategy::None,
        collected_at: chrono::Utc::now(),
        warnings: vec![reason.clone()],
        sample_status: Some(SampleStatus::Skipped { reason }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use dbsurveyor_core::{
        DatabaseSchema, DatabaseType, Result,
        adapters::{AdapterFeature, ConnectionConfig},
        error::DbSurveyorError,
        models::{DatabaseInfo, SamplingStrategy},
    };
    use serde_json::json;
    use std::{collections::VecDeque, sync::Mutex};

    struct MockAdapter {
        responses: Mutex<VecDeque<Result<TableSample>>>,
        sample_sizes: Mutex<Vec<u32>>,
    }

    impl MockAdapter {
        fn new(responses: Vec<Result<TableSample>>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
                sample_sizes: Mutex::new(Vec::new()),
            }
        }

        fn seen_sample_sizes(&self) -> Vec<u32> {
            self.sample_sizes.lock().expect("sample sizes lock").clone()
        }
    }

    #[async_trait]
    impl DatabaseAdapter for MockAdapter {
        async fn test_connection(&self) -> Result<()> {
            Ok(())
        }

        async fn collect_schema(&self) -> Result<DatabaseSchema> {
            Ok(DatabaseSchema::new(DatabaseInfo::new("mock".to_string())))
        }

        async fn sample_table(
            &self,
            _table_ref: TableRef<'_>,
            config: &SamplingConfig,
        ) -> Result<TableSample> {
            self.sample_sizes
                .lock()
                .expect("sample sizes lock")
                .push(config.sample_size);
            self.responses
                .lock()
                .expect("responses lock")
                .pop_front()
                .expect("mock response missing")
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        fn supports_feature(&self, _feature: AdapterFeature) -> bool {
            true
        }

        fn connection_config(&self) -> ConnectionConfig {
            ConnectionConfig::default()
        }
    }

    fn test_table() -> Table {
        Table {
            name: "users".to_string(),
            schema: Some("public".to_string()),
            columns: Vec::new(),
            primary_key: None,
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
            constraints: Vec::new(),
            comment: None,
            row_count: None,
        }
    }

    fn successful_sample(strategy: SamplingStrategy) -> TableSample {
        TableSample {
            table_name: "users".to_string(),
            schema_name: Some("public".to_string()),
            rows: vec![json!({"id": 1})],
            sample_size: 1,
            total_rows: Some(1),
            sampling_strategy: strategy,
            collected_at: chrono::Utc::now(),
            warnings: Vec::new(),
            sample_status: None,
        }
    }

    #[tokio::test]
    async fn test_orchestrator_success_path_marks_complete() {
        let adapter = MockAdapter::new(vec![Ok(successful_sample(SamplingStrategy::MostRecent {
            limit: 100,
        }))]);
        let config = SamplingConfig::default().with_sample_size(100);
        let run = SamplingOrchestrator::new(&adapter, &config)
            .run(&[test_table()])
            .await;

        assert_eq!(run.samples.len(), 1);
        assert!(matches!(
            run.samples[0].sample_status,
            Some(SampleStatus::Complete)
        ));
        assert!(run.warnings.is_empty());
        assert_eq!(adapter.seen_sample_sizes(), vec![100]);
    }

    #[tokio::test]
    async fn test_orchestrator_retry_success_marks_partial_retry() {
        let adapter = MockAdapter::new(vec![
            Err(DbSurveyorError::configuration("first attempt failed")),
            Ok(successful_sample(SamplingStrategy::MostRecent {
                limit: 50,
            })),
        ]);
        let config = SamplingConfig::default().with_sample_size(100);
        let run = SamplingOrchestrator::new(&adapter, &config)
            .run(&[test_table()])
            .await;

        assert_eq!(adapter.seen_sample_sizes(), vec![100, 50]);
        assert_eq!(run.samples.len(), 1);
        assert!(matches!(
            run.samples[0].sample_status,
            Some(SampleStatus::PartialRetry {
                original_limit: 100
            })
        ));
        assert!(
            run.samples[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("retried with reduced limit 50"))
        );
        assert_eq!(run.warnings, run.samples[0].warnings);
    }

    #[tokio::test]
    async fn test_orchestrator_retry_failure_marks_skipped() {
        let adapter = MockAdapter::new(vec![
            Err(DbSurveyorError::configuration("first attempt failed")),
            Err(DbSurveyorError::configuration("retry failed")),
        ]);
        let config = SamplingConfig::default().with_sample_size(80);
        let run = SamplingOrchestrator::new(&adapter, &config)
            .run(&[test_table()])
            .await;

        assert_eq!(adapter.seen_sample_sizes(), vec![80, 40]);
        assert_eq!(run.samples.len(), 1);
        assert!(matches!(
            run.samples[0].sample_status,
            Some(SampleStatus::Skipped { .. })
        ));
        assert!(
            !run.samples[0].warnings.is_empty(),
            "skipped sample should include a reason warning"
        );
        assert_eq!(run.warnings, run.samples[0].warnings);
    }

    #[tokio::test]
    async fn test_orchestrator_unordered_fallback_adds_warning() {
        let adapter = MockAdapter::new(vec![Ok(successful_sample(SamplingStrategy::Random {
            limit: 100,
        }))]);
        let config = SamplingConfig::default().with_sample_size(100);
        let run = SamplingOrchestrator::new(&adapter, &config)
            .run(&[test_table()])
            .await;

        assert_eq!(run.samples.len(), 1);
        assert!(matches!(
            run.samples[0].sample_status,
            Some(SampleStatus::Complete)
        ));
        assert!(
            run.samples[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("random sampling fallback"))
        );
        assert_eq!(run.warnings, run.samples[0].warnings);
    }

    #[tokio::test]
    async fn test_orchestrator_preserves_adapter_supplied_status_on_first_success() {
        let adapter = MockAdapter::new(vec![Ok(TableSample {
            sample_status: Some(SampleStatus::Skipped {
                reason: "adapter-level skip".to_string(),
            }),
            ..successful_sample(SamplingStrategy::None)
        })]);
        let config = SamplingConfig::default().with_sample_size(100);
        let run = SamplingOrchestrator::new(&adapter, &config)
            .run(&[test_table()])
            .await;

        assert!(matches!(
            run.samples[0].sample_status,
            Some(SampleStatus::Skipped { .. })
        ));
    }
}
