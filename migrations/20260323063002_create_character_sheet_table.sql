-- Add migration script here
CREATE TABLE IF NOT EXISTS character_sheets (
    id VARCHAR PRIMARY KEY,

    -- Meta
    meta JSONB NOT NULL,

    -- Identity
    identity JSONB NOT NULL,

    -- Progression
    progression JSONB NOT NULL,

    -- Combat
    combat JSONB NOT NULL,

    -- Nested blocks (all JSONB)
    abilities_block JSONB NOT NULL,
    skills_block JSONB NOT NULL,
    magic JSONB NOT NULL,
    inventory JSONB NOT NULL,
    traits JSONB NOT NULL,
    notes JSONB NOT NULL,

    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Index on primary key (discord_id)
CREATE INDEX IF NOT EXISTS idx_character_sheets_id ON character_sheets(id);

-- Index on character name for get_character_by_name queries
CREATE INDEX IF NOT EXISTS idx_character_sheets_character_name 
    ON character_sheets USING GIN(identity jsonb_path_ops);

-- Index on character name as text for faster equality checks
CREATE INDEX IF NOT EXISTS idx_character_sheets_character_name_text 
    ON character_sheets((identity->>'characterName'));

-- Index on created_at for sorting/filtering
CREATE INDEX IF NOT EXISTS idx_character_sheets_created_at ON character_sheets(created_at DESC);

-- Index on updated_at for finding recently modified characters
CREATE INDEX IF NOT EXISTS idx_character_sheets_updated_at ON character_sheets(updated_at DESC);