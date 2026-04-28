-- Add migration script here
CREATE TABLE story (
    summary TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (updated_at)
);

CREATE TABLE dialogues (
    dialogue TEXT NOT NULL,
    author_name TEXT NOT NULL,
    author_character TEXT NOT NULL,
    author_discord_id TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (updated_at)
);