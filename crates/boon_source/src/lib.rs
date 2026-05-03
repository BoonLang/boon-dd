use boon_dd::{GeneratedSourceEvent, GeneratedSourceEventPayload, SourceId};

pub fn static_empty(source_id: impl Into<String>) -> GeneratedSourceEvent {
    GeneratedSourceEvent::Static {
        source_id: SourceId(source_id.into()),
        payload: GeneratedSourceEventPayload::EmptyRecord,
    }
}
