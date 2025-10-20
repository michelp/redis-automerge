-- Create table for storing Automerge document changes
-- This table serves as an append-only log of all document changes

CREATE TABLE IF NOT EXISTS document_changes (
    id BIGSERIAL PRIMARY KEY,
    document_uuid UUID NOT NULL,
    change_data BYTEA NOT NULL,
    inserted_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create index on document_uuid for fast lookups by document
CREATE INDEX idx_document_changes_document_uuid ON document_changes(document_uuid);

-- Create index on inserted_at for temporal queries
CREATE INDEX idx_document_changes_inserted_at ON document_changes(inserted_at);

-- Create composite index for document + time range queries
CREATE INDEX idx_document_changes_uuid_time ON document_changes(document_uuid, inserted_at);

-- Add comment for documentation
COMMENT ON TABLE document_changes IS 'Append-only log of all Automerge document changes consumed from Redis';
COMMENT ON COLUMN document_changes.id IS 'Auto-incrementing primary key';
COMMENT ON COLUMN document_changes.document_uuid IS 'UUID of the document this change belongs to';
COMMENT ON COLUMN document_changes.change_data IS 'Binary Automerge change data';
COMMENT ON COLUMN document_changes.inserted_at IS 'Timestamp when this change was archived to PostgreSQL';
