use crate::intern::StringInterner;
use crate::producer::{Record, RecordBatcher};
use bytes::{BufMut, BytesMut};
use prost::Message;
use stream::Reducer;
use tracing::info;

pub(crate) mod proto {
    include!(concat!(env!("OUT_DIR"), "/aws.kinesis.rs"));
}

pub(crate) struct RecordAggregator {
    inner: RecordBatcher,
}

impl RecordAggregator {
    pub fn new(max_bytes: usize, max_records: usize) -> RecordAggregator {
        // Defaults from KPL
        RecordAggregator {
            inner: RecordBatcher::new(max_bytes, max_records),
        }
    }

    fn aggregate(&self, records: &[Record]) -> proto::AggregatedRecord {
        let mut intern = StringInterner::new();
        let records = records
            .iter()
            .map(|record| proto::Record {
                partition_key_index: intern.intern(&record.partition_key),
                data: record.data.clone(),
                ..Default::default()
            })
            .collect();

        proto::AggregatedRecord {
            records,
            partition_key_table: intern.take(),
            ..Default::default()
        }
    }
}

impl Reducer for RecordAggregator {
    type Item = Record;

    type Output = Record;

    fn try_push(&mut self, item: Record) -> Option<Record> {
        self.inner.try_push(item)
    }

    fn take(&mut self) -> Option<Record> {
        let records = self.inner.take()?;
        let partition_key = records[0].partition_key.clone();
        let predicted_shard_id = records[0].predicted_shard_id.clone();

        let aggregated = self.aggregate(&records);

        let capacity = aggregated.encoded_len() + 20;
        let mut buf = BytesMut::with_capacity(capacity);
        buf.put_slice(&[0xF3, 0x89, 0x9A, 0xC2]);

        aggregated.encode(&mut buf).unwrap();

        let checksum = md5::compute(&buf[4..]);

        buf.put_slice(&checksum.0);

        info!(
            capacity,
            len = buf.len(),
            ?checksum,
            "produced aggregated record"
        );

        Some(Record {
            partition_key,
            data: buf.freeze(),
            predicted_shard_id,
            acker: None,
            children: records,
        })
    }

    fn empty(&self) -> bool {
        self.inner.empty()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_proto() {
        let mut aggregated = proto::AggregatedRecord::default();
        aggregated.partition_key_table.push("Test".to_string());

        let mut record = proto::Record::default();
        record.partition_key_index = 0;
        aggregated.records.push(record);
    }
}
