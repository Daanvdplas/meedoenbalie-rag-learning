CREATE TABLE store_meta (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    structure TEXT NOT NULL
);
CREATE TABLE corpus_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    identity_json TEXT NOT NULL
);
CREATE TABLE sections (
    source_id TEXT NOT NULL,
    section_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    source_date TEXT NOT NULL,
    source_title TEXT NOT NULL,
    heading TEXT NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY(source_id, section_id)
);
CREATE INDEX sections_position ON sections(position);
CREATE TABLE section_vectors (
    source_id TEXT NOT NULL,
    section_id TEXT NOT NULL,
    vector BLOB NOT NULL,
    PRIMARY KEY(source_id, section_id),
    FOREIGN KEY(source_id, section_id)
        REFERENCES sections(source_id, section_id) ON DELETE CASCADE
);
CREATE TABLE embedding_cache (
    identity_sha256 TEXT NOT NULL,
    input_sha256 TEXT NOT NULL,
    dimensions INTEGER NOT NULL CHECK(dimensions > 0),
    vector BLOB NOT NULL,
    PRIMARY KEY(identity_sha256, input_sha256, dimensions)
);
