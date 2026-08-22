-- Cos72 skeleton schema, in its OWN database (cos72.db), physically separate
-- from Sin90's sin90.db and from the kernel's agent24.db.
--
-- ONE table, and deliberately generic. ME-4's job is to prove the boundary — a
-- second domain OS mounts, routes, stores and emits without the kernel knowing
-- it exists. Inventing a community-OS domain model here would be guessing at
-- someone else's product; the real Cos72 schema belongs to MushroomDAO/Cos72.
CREATE TABLE cos72_entries (
    id         TEXT PRIMARY KEY,
    text       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
