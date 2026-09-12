CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
);

CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE subjects (
    id TEXT PRIMARY KEY,
    identity_version INTEGER NOT NULL,
    origin TEXT,
    head TEXT,
    encoding TEXT NOT NULL DEFAULT 'meno-subject-v1',
    captured_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE claims (
    id TEXT PRIMARY KEY,
    statement TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('draft', 'frozen', 'retired')),
    origin_kind TEXT NOT NULL CHECK (origin_kind IN ('human', 'imported', 'agent')),
    origin_actor TEXT,
    origin_source TEXT,
    policy_id TEXT,
    current_revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE claim_revisions (
    claim_id TEXT NOT NULL REFERENCES claims(id),
    revision INTEGER NOT NULL,
    statement TEXT NOT NULL,
    state TEXT NOT NULL,
    origin_kind TEXT NOT NULL,
    origin_actor TEXT,
    origin_source TEXT,
    policy_id TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (claim_id, revision)
);

CREATE TABLE policies (
    id TEXT PRIMARY KEY,
    claim_id TEXT NOT NULL REFERENCES claims(id),
    version INTEGER NOT NULL,
    body_json TEXT NOT NULL,
    current_revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE policy_revisions (
    policy_id TEXT NOT NULL REFERENCES policies(id),
    revision INTEGER NOT NULL,
    version INTEGER NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (policy_id, revision)
);

CREATE TABLE artifacts (
    sha256 TEXT PRIMARY KEY,
    media_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    relative_path TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE evidence (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    subject_id TEXT NOT NULL REFERENCES subjects(id),
    envelope_json TEXT NOT NULL,
    integrity_alg TEXT NOT NULL,
    integrity_digest TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE observations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    evidence_id TEXT NOT NULL REFERENCES evidence(id),
    ordinal INTEGER NOT NULL,
    type TEXT NOT NULL,
    fields_json TEXT NOT NULL,
    UNIQUE (evidence_id, ordinal)
);

CREATE TABLE evidence_artifacts (
    evidence_id TEXT NOT NULL REFERENCES evidence(id),
    sha256 TEXT NOT NULL REFERENCES artifacts(sha256),
    PRIMARY KEY (evidence_id, sha256)
);

CREATE TABLE claim_evidence (
    claim_id TEXT NOT NULL REFERENCES claims(id),
    evidence_id TEXT NOT NULL REFERENCES evidence(id),
    relation TEXT NOT NULL CHECK (relation IN ('support', 'contradict', 'related')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (claim_id, evidence_id)
);

CREATE TABLE verdicts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    claim_id TEXT NOT NULL REFERENCES claims(id),
    subject_id TEXT NOT NULL REFERENCES subjects(id),
    verdict TEXT NOT NULL CHECK (verdict IN ('proven', 'disproven', 'unknown')),
    evaluation_version INTEGER NOT NULL,
    subject_identity_version INTEGER NOT NULL,
    explanation_json TEXT NOT NULL,
    evaluated_at TEXT NOT NULL
);

CREATE TABLE connections (
    id TEXT PRIMARY KEY,
    adapter TEXT NOT NULL,
    name TEXT NOT NULL,
    config_json TEXT NOT NULL,
    can_collect INTEGER NOT NULL DEFAULT 1,
    can_invoke INTEGER NOT NULL DEFAULT 0,
    side_effect_level TEXT NOT NULL DEFAULT 'consequential',
    requires_confirmation INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE adapter_runs (
    id TEXT PRIMARY KEY,
    adapter TEXT NOT NULL,
    connection_id TEXT REFERENCES connections(id),
    subject_id TEXT REFERENCES subjects(id),
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    error TEXT
);

CREATE TABLE audit_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at TEXT NOT NULL,
    actor TEXT,
    action TEXT NOT NULL,
    entity_kind TEXT,
    entity_id TEXT,
    payload_json TEXT
);

CREATE INDEX idx_evidence_subject_id ON evidence(subject_id);
CREATE INDEX idx_evidence_kind ON evidence(kind);
CREATE INDEX idx_verdicts_claim_id_subject_id ON verdicts(claim_id, subject_id);
CREATE INDEX idx_observations_evidence_id ON observations(evidence_id);
CREATE INDEX idx_audit_events_at ON audit_events(at);

CREATE TRIGGER evidence_immutable_update
BEFORE UPDATE ON evidence
BEGIN
    SELECT RAISE(ABORT, 'evidence is immutable');
END;

CREATE TRIGGER audit_events_append_only_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events are append-only');
END;

CREATE TRIGGER audit_events_append_only_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit_events are append-only');
END;
