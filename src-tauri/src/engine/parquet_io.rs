use std::fs::File;
use std::path::Path;

use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use super::types::FileMetadata;

pub fn read_parquet(path: &Path) -> Result<(RecordBatch, FileMetadata), String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;

    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| format!("Failed to create parquet reader: {}", e))?;

    let metadata = builder.metadata().file_metadata().clone();

    let compression = Compression::SNAPPY;

    let row_group_size = metadata.num_rows() as usize;

    let reader = builder
        .build()
        .map_err(|e| format!("Failed to build parquet reader: {}", e))?;

    let mut all_batches: Vec<RecordBatch> = Vec::new();
    for batch_result in reader {
        let batch = batch_result.map_err(|e| format!("Failed to read batch: {}", e))?;
        all_batches.push(batch);
    }

    let batch = if all_batches.is_empty() {
        return Err("No data in parquet file".to_string());
    } else if all_batches.len() == 1 {
        all_batches.remove(0)
    } else {
        let schema = all_batches[0].schema();
        let num_cols = all_batches[0].num_columns();

        let mut columns = Vec::with_capacity(num_cols);
        for col_idx in 0..num_cols {
            let arrays: Vec<&dyn arrow::array::Array> = all_batches.iter()
                .map(|b| b.column(col_idx).as_ref())
                .collect();
            let concatenated = arrow::compute::concat(&arrays)
                .map_err(|e| format!("Failed to concatenate batches: {}", e))?;
            columns.push(concatenated);
        }
        RecordBatch::try_new(schema, columns)
            .map_err(|e| format!("Failed to create combined batch: {}", e))?
    };

    let file_meta = FileMetadata {
        path: Some(path.to_path_buf()),
        compression,
        row_group_size,
        data_page_size: None,
    };

    Ok((batch, file_meta))
}

pub fn write_parquet(
    path: &Path,
    batch: &RecordBatch,
    file_meta: Option<&FileMetadata>,
) -> Result<(), String> {
    let meta = file_meta.cloned().unwrap_or_default();

    let mut props_builder = WriterProperties::builder()
        .set_compression(meta.compression);

    if let Some(page_size) = meta.data_page_size {
        props_builder = props_builder.set_data_page_size_limit(page_size);
    }

    let props = props_builder.build();

    let file = File::create(path).map_err(|e| format!("Failed to create file: {}", e))?;

    let schema = batch.schema();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .map_err(|e| format!("Failed to create parquet writer: {}", e))?;

    writer
        .write(batch)
        .map_err(|e| format!("Failed to write batch: {}", e))?;

    writer
        .close()
        .map_err(|e| format!("Failed to close parquet writer: {}", e))?;

    Ok(())
}
