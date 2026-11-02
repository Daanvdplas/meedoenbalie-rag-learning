CREATE TABLE activities (
    activity_id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    source_date TEXT NOT NULL
);
CREATE TABLE activity_section_links (
    activity_id TEXT NOT NULL REFERENCES activities(activity_id) ON DELETE CASCADE,
    section_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    PRIMARY KEY(activity_id, position),
    UNIQUE(activity_id, section_id),
    FOREIGN KEY(activity_id, section_id)
        REFERENCES sections(source_id, section_id) ON DELETE CASCADE
);
CREATE TABLE activity_schedules (
    activity_id TEXT PRIMARY KEY NOT NULL REFERENCES activities(activity_id) ON DELETE CASCADE,
    start_minutes INTEGER NOT NULL CHECK(start_minutes >= 0 AND start_minutes < 1440),
    end_minutes INTEGER NOT NULL CHECK(end_minutes > start_minutes AND end_minutes <= 1440)
);
CREATE TABLE activity_weekdays (
    activity_id TEXT NOT NULL REFERENCES activity_schedules(activity_id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    weekday TEXT NOT NULL CHECK(weekday IN ('monday','tuesday','wednesday','thursday','friday','saturday','sunday')),
    PRIMARY KEY(activity_id, position),
    UNIQUE(activity_id, weekday)
);
CREATE TABLE activity_environments (
    activity_id TEXT PRIMARY KEY NOT NULL REFERENCES activities(activity_id) ON DELETE CASCADE,
    environment TEXT NOT NULL CHECK(environment IN ('indoor','outdoor'))
);
CREATE TABLE activity_fact_evidence (
    activity_id TEXT NOT NULL REFERENCES activities(activity_id) ON DELETE CASCADE,
    field_name TEXT NOT NULL CHECK(field_name IN ('schedule','environment')),
    position INTEGER NOT NULL CHECK(position >= 0),
    source_id TEXT NOT NULL,
    section_id TEXT NOT NULL,
    source_date TEXT NOT NULL,
    quote TEXT NOT NULL,
    PRIMARY KEY(activity_id, field_name, position),
    FOREIGN KEY(source_id, section_id)
        REFERENCES sections(source_id, section_id) ON DELETE CASCADE
);
